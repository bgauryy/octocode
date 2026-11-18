//! One lazy selection flow for tool requests and read-only auth inspection.
mod backend;
mod call;
use super::super::login;
use super::{StoredCredentials, configured_credential_host, normalize_host};
use crate::config::ConfigOutput;
use backend::{AuthBackend, SystemBackend};
pub use call::CallSelection;
use octocode_github::{CredentialSource, ProviderError, ProviderErrorKind, ResolvedCredential};
use octocode_io::budget::{BudgetStop, RequestBudget};
use secrecy::ExposeSecret;
use std::{
    collections::HashSet,
    future::Future,
    sync::{Arc, Mutex},
    time::SystemTime,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Inspect,
    Request,
}

#[derive(Clone)]
pub struct AuthSelection {
    pub credential: ResolvedCredential,
    pub username: Option<String>,
}
impl AuthSelection {
    /// Preserve the CLI's existing public labels independently of internal sources.
    pub fn source_label(&self) -> &'static str {
        match self.credential.source {
            CredentialSource::Override | CredentialSource::Environment => "env",
            CredentialSource::Storage => "platform",
            CredentialSource::Home => "octocode-storage",
            CredentialSource::GhCli => "gh-cli",
        }
    }
    pub fn token(&self) -> &str {
        self.credential.expose_secret()
    }
}

pub struct Authentication {
    config: Arc<ConfigOutput>,
    /// Host partitions of the tokens GitHub rejected in this process. A
    /// rejected stored or `gh` token is skipped, so the next request falls
    /// back as a fresh selection would instead of sending it again.
    rejected: Mutex<HashSet<octocode_github::Partition>>,
    /// The wall clock token expiry is judged by; tests move it forward
    /// instead of sleeping past a refresh time.
    now: fn() -> SystemTime,
}
impl Authentication {
    pub fn new(config: Arc<ConfigOutput>) -> Self {
        Self {
            config,
            rejected: Mutex::default(),
            now: SystemTime::now,
        }
    }
    /// Whether `stored`'s access token is due for a refresh now.
    fn expired(&self, stored: &StoredCredentials) -> bool {
        login::refresh_due(stored).is_some_and(|due| (self.now)() >= due)
    }
    /// GitHub rejected `credential` for `host` (HTTP 401): subsequent
    /// requests skip it while selecting from the current credential sources.
    pub fn reject(&self, host: &str, credential: Option<&ResolvedCredential>) {
        let host = normalize_host(host);
        if let Some(credential) = credential {
            self.rejected
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(octocode_github::Partition::new(
                    host.as_str(),
                    Some(credential.secret()),
                ));
        }
    }
    fn is_rejected(&self, host: &str, token: &str) -> bool {
        let token = secrecy::SecretString::from(token);
        self.rejected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&octocode_github::Partition::new(host, Some(&token)))
    }
    pub async fn resolve(
        &self,
        host: &str,
        mode: AuthMode,
        budget: &RequestBudget,
    ) -> Result<Option<AuthSelection>, ProviderError> {
        self.resolve_with(
            host,
            mode,
            budget,
            &SystemBackend {
                config: &self.config,
            },
        )
        .await
    }
    /// Inspect local credentials afresh without token refresh or network access.
    /// A credential this runtime rejected cannot authorize a saved response.
    async fn inspect_identity_with(
        &self,
        host: &str,
        budget: &RequestBudget,
        backend: &impl AuthBackend,
    ) -> Result<String, ProviderError> {
        let selection = self
            .resolve_with(host, AuthMode::Inspect, budget, backend)
            .await?;
        self.selection_identity(host, selection.as_ref())
    }

    /// The identity of `selection`, refused when this runtime saw GitHub reject it.
    fn selection_identity(
        &self,
        host: &str,
        selection: Option<&AuthSelection>,
    ) -> Result<String, ProviderError> {
        if let Some(selected) = selection {
            self.check_not_rejected(host, selected.token())?;
        }
        Ok(octocode_github::Partition::new(
            host,
            selection.map(|selected| selected.credential.secret()),
        )
        .identity()
        .to_owned())
    }

    fn check_not_rejected(&self, host: &str, token: &str) -> Result<(), ProviderError> {
        if self.is_rejected(host, token) {
            return Err(ProviderError::new(
                ProviderErrorKind::Authentication,
                "The current GitHub credential was rejected.",
            ));
        }
        Ok(())
    }

    async fn resolve_with(
        &self,
        host: &str,
        mode: AuthMode,
        budget: &RequestBudget,
        backend: &impl AuthBackend,
    ) -> Result<Option<AuthSelection>, ProviderError> {
        budget.check().map_err(resolution_stopped)?;
        let host = normalize_host(host);
        if let Some(token) = self.config.token.as_ref()
            && configured_credential_host(&self.config.resolved.github.api_url)
                .is_some_and(|configured| configured == host)
        {
            if mode == AuthMode::Request {
                // An explicit environment token never falls back to another account.
                // Refuse it before provider work, just as saved identity checks do.
                self.check_not_rejected(&host, token.token())?;
            }
            return Ok(Some(AuthSelection {
                credential: ResolvedCredential::new(token.token(), CredentialSource::Environment),
                username: None,
            }));
        }
        self.resolve_stored(&host, mode, budget, backend).await
    }
    async fn resolve_stored(
        &self,
        host: &str,
        mode: AuthMode,
        budget: &RequestBudget,
        backend: &impl AuthBackend,
    ) -> Result<Option<AuthSelection>, ProviderError> {
        let host = host.to_owned();
        // Keychain calls are blocking OS operations. Await their worker to completion,
        // then check the budget before starting any further I/O. Expiry is
        // checked before rejection: GitHub rejecting an expired token is a
        // reason to refresh it, not to skip it.
        let stored = match backend.load(&host).await {
            Ok(Some((stored, _)))
                if mode == AuthMode::Request
                    && !self.expired(&stored)
                    && self.is_rejected(&host, &stored.token.token) =>
            {
                Ok(None)
            }
            stored => stored,
        };
        budget.check().map_err(resolution_stopped)?;
        let mut storage_error = None;
        match stored {
            Ok(Some((stored, source))) if mode == AuthMode::Inspect || !self.expired(&stored) => {
                return Ok(Some(stored_selection(stored, source)));
            }
            Ok(Some((_, source))) => {
                let client_id = Some(login::client_id_for_host(
                    &host,
                    self.config
                        .env_value(crate::config::environment::OCTOCODE_GITHUB_CLIENT_ID),
                ))
                .filter(|id| !id.is_empty());
                let refreshed = match client_id {
                    Some(client_id) => {
                        bounded(budget, backend.refresh(&host, client_id, source)).await
                    }
                    None => Err(ProviderError::new(
                        ProviderErrorKind::Configuration,
                        format!(
                            "{} is required to refresh GitHub Enterprise credentials",
                            crate::config::environment::OCTOCODE_GITHUB_CLIENT_ID
                        ),
                    )),
                };
                match refreshed {
                    Ok(Some(stored)) => return Ok(Some(stored_selection(stored, source))),
                    Ok(None) => {} // Deleted while waiting: never reuse the stale token.
                    Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
                    Err(error) => storage_error = Some(error),
                }
            }
            Ok(None) => {}
            Err(error) => storage_error = Some(error),
        }
        budget.check().map_err(resolution_stopped)?;
        let refused = |token: &str| mode == AuthMode::Request && self.is_rejected(&host, token);
        if let Some(token) = backend.gh(&host, budget, &refused).await?
            && !refused(token.expose_secret())
        {
            return Ok(Some(AuthSelection {
                credential: ResolvedCredential::new(token, CredentialSource::GhCli),
                username: None,
            }));
        }
        match storage_error {
            // No token and no usable store (headless, CI, sandboxed home):
            // requests run anonymously instead of failing.
            Some(error)
                if mode == AuthMode::Request
                    && error.kind == ProviderErrorKind::CredentialStoreUnavailable =>
            {
                let notice = anonymous_notice(&host);
                octocode_io::notice::raise(
                    octocode_io::notice::kind::GITHUB_ANONYMOUS,
                    notice.clone(),
                );
                Ok(None)
            }
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}
fn anonymous_notice(host: &str) -> String {
    format!(
        "[octocode] warning: no GitHub credential for {host} and the secure credential store is unavailable; GitHub tools run unauthenticated (60 requests/hour). Set GITHUB_TOKEN or run octocode auth login for a higher limit."
    )
}
fn stored_selection(stored: StoredCredentials, source: CredentialSource) -> AuthSelection {
    AuthSelection {
        credential: ResolvedCredential::new(stored.token.token, source),
        username: (!stored.username.is_empty()).then_some(stored.username),
    }
}

/// A budget stop met while resolving a credential.
pub(super) fn resolution_stopped(stop: BudgetStop) -> ProviderError {
    match stop {
        BudgetStop::Cancelled => ProviderError::new(
            ProviderErrorKind::Cancelled,
            "GitHub credential resolution cancelled",
        ),
        BudgetStop::Deadline => ProviderError::new(
            ProviderErrorKind::Timeout,
            "GitHub credential resolution exceeded the request budget",
        ),
    }
}
/// `operation`'s result, unless `budget` stops it first. The operation is
/// boxed once, here: the wait holds it and a timeout around it, and every
/// future that awaits this one would otherwise carry that inline (a token
/// refresh is several kilobytes).
pub(super) fn bounded<T>(
    budget: &RequestBudget,
    operation: impl Future<Output = Result<T, ProviderError>>,
) -> impl Future<Output = Result<T, ProviderError>> {
    let wait = budget.wait(Box::pin(operation));
    async move { wait.await.map_err(resolution_stopped)? }
}

#[cfg(test)]
mod tests;
