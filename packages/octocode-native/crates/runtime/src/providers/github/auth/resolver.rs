//! One lazy selection flow for tool requests and read-only auth inspection.
use super::super::{ProviderError, ProviderErrorKind, RequestContext, login};
use super::{
    CredentialSource, ResolvedCredential, StoredCredentials, configured_credential_host,
    normalize_host,
};
use crate::config::ConfigOutput;
use std::{future::Future, sync::Arc, time::Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Inspect,
    Request,
}

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
}
impl Authentication {
    pub fn new(config: Arc<ConfigOutput>) -> Self {
        Self { config }
    }
    pub async fn resolve(
        &self,
        host: &str,
        mode: AuthMode,
        budget: &RequestContext,
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
    async fn resolve_with(
        &self,
        host: &str,
        mode: AuthMode,
        budget: &RequestContext,
        backend: &impl AuthBackend,
    ) -> Result<Option<AuthSelection>, ProviderError> {
        check_budget(budget)?;
        let host = normalize_host(host);
        if let Some(token) = budget.override_token.as_ref() {
            use secrecy::ExposeSecret;
            if !token.expose_secret().trim().is_empty() {
                return Ok(Some(AuthSelection {
                    credential: ResolvedCredential::new(token.clone(), CredentialSource::Override),
                    username: None,
                }));
            }
        }
        if let Some(token) = self.config.token.as_ref()
            && configured_credential_host(&self.config.resolved.github.api_url)
                .is_some_and(|configured| configured == host)
        {
            return Ok(Some(AuthSelection {
                credential: ResolvedCredential::new(token.token(), CredentialSource::Environment),
                username: None,
            }));
        }
        // Keychain calls are blocking OS operations. Await their worker to completion,
        // then check the budget before starting any further I/O.
        let stored = backend.load(&host).await;
        check_budget(budget)?;
        let mut storage_error = None;
        match stored {
            Ok(Some((stored, source)))
                if mode == AuthMode::Inspect || !login::is_token_expired(&stored) =>
            {
                return Ok(Some(from_stored(stored, source)));
            }
            Ok(Some((_, source))) => {
                let client_id = self
                    .config
                    .env_value("OCTOCODE_GITHUB_CLIENT_ID")
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .or((host == "github.com").then_some(login::GITHUB_APP_CLIENT_ID));
                let refreshed = match client_id {
                    Some(client_id) => {
                        bounded(budget, backend.refresh(&host, client_id, source)).await
                    }
                    None => Err(ProviderError::new(
                        ProviderErrorKind::Configuration,
                        "OCTOCODE_GITHUB_CLIENT_ID is required to refresh GitHub Enterprise credentials",
                    )),
                };
                match refreshed {
                    Ok(Some(stored)) => return Ok(Some(from_stored(stored, source))),
                    Ok(None) => {} // Deleted while waiting: never reuse the stale token.
                    Err(error) if is_budget_error(&error) => return Err(error),
                    Err(error) => storage_error = Some(error),
                }
            }
            Ok(None) => {}
            Err(error) => storage_error = Some(error),
        }
        check_budget(budget)?;
        if let Some(token) = backend.gh(&host, budget).await? {
            return Ok(Some(AuthSelection {
                credential: ResolvedCredential::new(token, CredentialSource::GhCli),
                username: None,
            }));
        }
        match storage_error {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}
fn from_stored(stored: StoredCredentials, source: CredentialSource) -> AuthSelection {
    AuthSelection {
        credential: ResolvedCredential::new(stored.token.token, source),
        username: (!stored.username.is_empty()).then_some(stored.username),
    }
}

trait AuthBackend {
    async fn load(
        &self,
        host: &str,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError>;
    async fn refresh(
        &self,
        host: &str,
        client_id: &str,
        source: CredentialSource,
    ) -> Result<Option<StoredCredentials>, ProviderError>;
    async fn gh(
        &self,
        host: &str,
        budget: &RequestContext,
    ) -> Result<Option<secrecy::SecretString>, ProviderError>;
}
struct SystemBackend<'a> {
    config: &'a ConfigOutput,
}
impl AuthBackend for SystemBackend<'_> {
    async fn load(
        &self,
        host: &str,
    ) -> Result<Option<(StoredCredentials, CredentialSource)>, ProviderError> {
        let host = host.to_owned();
        let store = super::CredentialStore::new(&self.config.home);
        tokio::task::spawn_blocking(move || store.load(&host))
            .await
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::CredentialStoreUnavailable,
                    "credential resolution worker failed",
                )
            })?
    }
    async fn refresh(
        &self,
        host: &str,
        client_id: &str,
        source: CredentialSource,
    ) -> Result<Option<StoredCredentials>, ProviderError> {
        login::refresh_stored_in_store(
            host,
            client_id,
            &super::CredentialStore::new(&self.config.home),
            source,
        )
        .await
    }
    async fn gh(
        &self,
        host: &str,
        budget: &RequestContext,
    ) -> Result<Option<secrecy::SecretString>, ProviderError> {
        super::discovery::gh_token(host, self.config.effective_env(), budget).await
    }
}
pub(super) fn check_budget(budget: &RequestContext) -> Result<(), ProviderError> {
    if budget.cancellation.is_cancelled() {
        return Err(ProviderError::new(
            ProviderErrorKind::Cancelled,
            "GitHub credential resolution cancelled",
        ));
    }
    if Instant::now() >= budget.deadline {
        return Err(ProviderError::new(
            ProviderErrorKind::Timeout,
            "GitHub credential resolution exceeded the request budget",
        ));
    }
    Ok(())
}
fn is_budget_error(error: &ProviderError) -> bool {
    matches!(
        error.kind,
        ProviderErrorKind::Cancelled | ProviderErrorKind::Timeout
    )
}
pub(super) async fn bounded<T>(
    budget: &RequestContext,
    operation: impl Future<Output = Result<T, ProviderError>>,
) -> Result<T, ProviderError> {
    check_budget(budget)?;
    tokio::select! {
        biased;
        _ = budget.cancellation.cancelled() => Err(ProviderError::new(ProviderErrorKind::Cancelled, "GitHub credential resolution cancelled")),
        _ = tokio::time::sleep_until(budget.deadline.into()) => Err(ProviderError::new(ProviderErrorKind::Timeout, "GitHub credential resolution exceeded the request budget")),
        result = operation => result,
    }
}

#[cfg(test)]
mod tests;
