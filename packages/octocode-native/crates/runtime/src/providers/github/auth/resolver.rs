//! One lazy selection flow for tool requests and read-only auth inspection.
mod backend;
use super::super::login;
use super::{StoredCredentials, configured_credential_host, normalize_host};
use crate::config::ConfigOutput;
use backend::{AuthBackend, SystemBackend};
use octocode_github::{CredentialSource, ProviderError, ProviderErrorKind, ResolvedCredential};
use octocode_io::budget::{BudgetStop, RequestBudget};
use secrecy::ExposeSecret;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

/// How long a request credential (or "no credential") is remembered: a
/// login, logout, `gh auth switch`, or token rotation made in another process
/// is picked up within it, without a restart of a long-lived host (an MCP
/// session, the CLI's warm server).
const MEMO_TTL: Duration = Duration::from_secs(60);

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

/// A request credential remembered for one host.
struct Remembered {
    selection: Option<AuthSelection>,
    at: Instant,
    /// When a remembered stored credential must be refreshed; past it the
    /// entry is a miss, so the next request reloads and refreshes.
    refresh_due: Option<SystemTime>,
    /// The warning the selection was made with, said again by each request
    /// that reuses it, as a fresh selection would.
    notice: Option<String>,
}

pub struct Authentication {
    config: Arc<ConfigOutput>,
    /// Request credentials resolve once per host per [`MEMO_TTL`]: a `gh`
    /// spawn or a store read per query row is pure overhead.
    memo: Mutex<HashMap<String, Remembered>>,
    /// Host partitions of the tokens GitHub rejected in this process. A
    /// rejected stored or `gh` token is skipped, so the next request falls
    /// back as a fresh selection would instead of sending it again.
    rejected: Mutex<HashSet<octocode_github::Partition>>,
    /// Request resolutions take turns per host: the rows of a cold batch
    /// wait for the first one's answer in the memo instead of each reading
    /// the store or spawning `gh`.
    flights: octocode_io::flight::KeyedFlight<String, ()>,
    /// The wall clock token expiry is judged by; tests move it forward
    /// instead of sleeping past a refresh time.
    now: fn() -> SystemTime,
}
impl Authentication {
    pub fn new(config: Arc<ConfigOutput>) -> Self {
        Self {
            config,
            memo: Mutex::default(),
            rejected: Mutex::default(),
            flights: octocode_io::flight::KeyedFlight::transient(),
            now: SystemTime::now,
        }
    }
    /// Whether `stored`'s access token is due for a refresh now.
    fn expired(&self, stored: &StoredCredentials) -> bool {
        login::refresh_due(stored).is_some_and(|due| (self.now)() >= due)
    }
    /// GitHub rejected `credential` for `host` (HTTP 401): drop the
    /// remembered selection so the next request selects again, skipping it.
    pub(crate) fn reject(&self, host: &str, credential: Option<&ResolvedCredential>) {
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
        self.memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&host);
    }
    fn is_rejected(&self, host: &str, token: &str) -> bool {
        let token = secrecy::SecretString::from(token);
        self.rejected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&octocode_github::Partition::new(host, Some(&token)))
    }
    fn remembered(&self, host: &str) -> Option<(Option<AuthSelection>, Option<String>)> {
        let memo = self
            .memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = memo.get(host)?;
        let live =
            entry.at.elapsed() < MEMO_TTL && entry.refresh_due.is_none_or(|due| (self.now)() < due);
        live.then(|| (entry.selection.clone(), entry.notice.clone()))
    }
    fn remember(&self, host: &str, resolved: &Resolved) {
        self.memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                host.to_owned(),
                Remembered {
                    selection: resolved.selection.clone(),
                    at: Instant::now(),
                    refresh_due: resolved.refresh_due,
                    notice: resolved.notice.clone(),
                },
            );
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
    pub(crate) async fn inspect_response_identity(
        &self,
        host: &str,
        budget: &RequestBudget,
    ) -> Result<String, ProviderError> {
        let selection = self.resolve(host, AuthMode::Inspect, budget).await?;
        if selection
            .as_ref()
            .is_some_and(|selected| self.is_rejected(host, selected.token()))
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Authentication,
                "The current GitHub credential was rejected.",
            ));
        }
        Ok(octocode_github::Partition::new(
            host,
            selection
                .as_ref()
                .map(|selected| selected.credential.secret()),
        )
        .identity()
        .to_owned())
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
            return Ok(Some(AuthSelection {
                credential: ResolvedCredential::new(token.token(), CredentialSource::Environment),
                username: None,
            }));
        }
        let ticket = (mode == AuthMode::Request).then(|| self.flights.join(host.clone()));
        let _turn = match &ticket {
            Some(ticket) => Some(
                budget
                    .wait(ticket.turn())
                    .await
                    .map_err(resolution_stopped)?,
            ),
            None => None,
        };
        if mode == AuthMode::Request
            && let Some((selection, notice)) = self.remembered(&host)
        {
            if let Some(notice) = notice {
                octocode_io::notice::raise(ANONYMOUS_NOTICE, notice);
            }
            return Ok(selection);
        }
        let resolved = self.resolve_stored(&host, mode, budget, backend).await?;
        if mode == AuthMode::Request {
            self.remember(&host, &resolved);
        }
        Ok(resolved.selection)
    }
    async fn resolve_stored(
        &self,
        host: &str,
        mode: AuthMode,
        budget: &RequestBudget,
        backend: &impl AuthBackend,
    ) -> Result<Resolved, ProviderError> {
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
                return Ok(Resolved::stored(stored, source));
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
                    Ok(Some(stored)) => return Ok(Resolved::stored(stored, source)),
                    Ok(None) => {} // Deleted while waiting: never reuse the stale token.
                    Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
                    Err(error) => storage_error = Some(error),
                }
            }
            Ok(None) => {}
            Err(error) => storage_error = Some(error),
        }
        budget.check().map_err(resolution_stopped)?;
        if let Some(token) = backend.gh(&host, budget).await?
            && !(mode == AuthMode::Request && self.is_rejected(&host, token.expose_secret()))
        {
            return Ok(Resolved::untimed(Some(AuthSelection {
                credential: ResolvedCredential::new(token, CredentialSource::GhCli),
                username: None,
            })));
        }
        match storage_error {
            // No token and no usable store (headless, CI, sandboxed home):
            // requests run anonymously instead of failing.
            Some(error)
                if mode == AuthMode::Request
                    && error.kind == ProviderErrorKind::CredentialStoreUnavailable =>
            {
                let notice = anonymous_notice(&host);
                octocode_io::notice::raise(ANONYMOUS_NOTICE, notice.clone());
                Ok(Resolved {
                    notice: Some(notice),
                    ..Resolved::untimed(None)
                })
            }
            Some(error) => Err(error),
            None => Ok(Resolved::untimed(None)),
        }
    }
}
/// The kind of [`anonymous_notice`]: said on stderr once per process
/// ([`octocode_io::notice`]).
const ANONYMOUS_NOTICE: &str = "github-anonymous";

fn anonymous_notice(host: &str) -> String {
    format!(
        "[octocode] warning: no GitHub credential for {host} and the secure credential store is unavailable; GitHub tools run unauthenticated (60 requests/hour). Set GITHUB_TOKEN or run octocode auth login for a higher limit."
    )
}
/// A selection and when it must be refreshed.
struct Resolved {
    selection: Option<AuthSelection>,
    refresh_due: Option<SystemTime>,
    /// A warning the selection was made with.
    notice: Option<String>,
}
impl Resolved {
    fn untimed(selection: Option<AuthSelection>) -> Self {
        Self {
            selection,
            refresh_due: None,
            notice: None,
        }
    }
    fn stored(stored: StoredCredentials, source: CredentialSource) -> Self {
        Self {
            notice: None,
            refresh_due: login::refresh_due(&stored),
            selection: Some(AuthSelection {
                credential: ResolvedCredential::new(stored.token.token, source),
                username: (!stored.username.is_empty()).then_some(stored.username),
            }),
        }
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
/// `operation`'s result, unless `budget` stops it first.
pub(super) async fn bounded<T>(
    budget: &RequestBudget,
    operation: impl Future<Output = Result<T, ProviderError>>,
) -> Result<T, ProviderError> {
    budget.wait(operation).await.map_err(resolution_stopped)?
}

#[cfg(test)]
mod tests;
