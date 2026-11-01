//! GitHub sign-in use cases an interface presents: the credential host, what
//! a login does (refresh, keep a live credential, or run the device flow),
//! the token status check, and logout. These decide; interfaces print.
use super::ToolRuntime;
use crate::providers::github::login::{self, TokenCheck};
use crate::providers::github::{
    AuthMode, AuthSelection, Authentication, CredentialStore, StoredCredentials,
    configured_credential_host,
};
use octocode_contract_types::ErrorCode;
use octocode_github::ProviderError;
use octocode_io::budget::RequestBudget;
use std::time::Duration;

/// How long a status check waits for credential resolution and for GitHub.
const STATUS_WAIT: Duration = Duration::from_secs(5);

/// The host and OAuth client a login or refresh uses.
pub struct LoginTarget {
    pub host: String,
    client_id: String,
}

/// What `auth login` does for a [`LoginTarget`].
pub enum LoginPlan {
    /// Exchange the stored refresh token; no device flow.
    Refresh,
    /// A live stored credential stays in place (`force` replaces it).
    AlreadyAuthenticated(Box<StoredCredentials>),
    /// Run the device flow. `env_override` names an environment token that
    /// will keep taking precedence over the credential this login saves.
    DeviceFlow { env_override: Option<String> },
}

/// Whether the active token works, as GitHub answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    /// No token is configured.
    None,
    /// GitHub accepted the token.
    Verified,
    /// GitHub could not be asked; the token stands unverified.
    Unverified,
    /// GitHub rejected the token (HTTP 401).
    Invalid,
}

impl Verification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Invalid => "invalid",
        }
    }

    pub fn authenticated(self) -> bool {
        matches!(self, Self::Verified | Self::Unverified)
    }
}

/// `auth status`: the active token's source and what GitHub says about it.
pub struct AuthStatus {
    pub host: String,
    pub source: String,
    pub verification: Verification,
    pub username: Option<String>,
}

impl ToolRuntime {
    /// The credential host of the configured GitHub API (`api.github.com`
    /// is `github.com`): status, login and logout all target it.
    #[must_use]
    pub fn github_credential_host(&self) -> String {
        configured_credential_host(&self.config().resolved.github.api_url)
            .unwrap_or_else(|| "github.com".into())
    }

    async fn auth_selection(&self, host: &str) -> Option<AuthSelection> {
        let auth = Authentication::new(std::sync::Arc::new(self.config().clone()));
        let budget = RequestBudget::with_timeout(STATUS_WAIT, 0);
        auth.resolve(host, AuthMode::Inspect, &budget)
            .await
            .ok()
            .flatten()
    }

    /// Where the active GitHub token comes from, without verifying it.
    pub async fn active_token_source(&self) -> Option<String> {
        let host = self.github_credential_host();
        self.auth_selection(&host)
            .await
            .map(|selection| selection.source_label().to_owned())
    }

    /// The active token and GitHub's answer for it.
    pub async fn auth_status(&self) -> AuthStatus {
        let host = self.github_credential_host();
        let selection = self.auth_selection(&host).await;
        let check = match &selection {
            Some(selected) => Some(
                login::verify_token(
                    &self.config().resolved.github.api_url,
                    selected.token(),
                    STATUS_WAIT,
                )
                .await,
            ),
            None => None,
        };
        let (verification, verified_login) = match check {
            None => (Verification::None, None),
            Some(TokenCheck::Valid(user)) => (Verification::Verified, user),
            Some(TokenCheck::Unverified) => (Verification::Unverified, None),
            Some(TokenCheck::Rejected) => (Verification::Invalid, None),
        };
        let username = verified_login.or_else(|| {
            verification
                .authenticated()
                .then(|| selection.as_ref().and_then(|s| s.username.clone()))
                .flatten()
        });
        AuthStatus {
            host,
            source: selection
                .as_ref()
                .map_or("none", AuthSelection::source_label)
                .to_owned(),
            verification,
            username,
        }
    }

    /// The host (`hostname` when given, else the configured one) and its
    /// OAuth client; an Enterprise host without a configured client id is
    /// the missing configuration this names.
    pub fn login_target(&self, hostname: Option<&str>) -> Result<LoginTarget, (ErrorCode, String)> {
        let host = hostname
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map_or_else(|| self.github_credential_host(), str::to_owned);
        let variable = crate::config::environment::OCTOCODE_GITHUB_CLIENT_ID;
        let client_id = login::client_id_for_host(&host, self.config().env_value(variable));
        if client_id.is_empty() {
            return Err((
                ErrorCode::MissingConfiguration,
                format!("{variable} is required for GitHub Enterprise device login and refresh."),
            ));
        }
        Ok(LoginTarget {
            host,
            client_id: client_id.to_owned(),
        })
    }

    fn credential_store(&self) -> CredentialStore {
        CredentialStore::new(&self.config().home)
    }

    /// What a login of `target` does: refresh when asked, keep a live stored
    /// credential unless `force`, else the device flow.
    #[must_use]
    pub fn login_plan(&self, target: &LoginTarget, force: bool, refresh: bool) -> LoginPlan {
        if refresh {
            return LoginPlan::Refresh;
        }
        let stored = self
            .credential_store()
            .load(&target.host)
            .ok()
            .flatten()
            .map(|(stored, _)| stored);
        if let Some(existing) = stored.filter(|stored| !force && !login::is_token_expired(stored)) {
            return LoginPlan::AlreadyAuthenticated(Box::new(existing));
        }
        // The resolved selection names the token the request path would use.
        let env_override = self
            .config()
            .token
            .as_ref()
            .map(crate::config::PrivateTokenSelection::source)
            .and_then(|source| source.strip_prefix("env:"))
            .map(str::to_owned);
        LoginPlan::DeviceFlow { env_override }
    }

    /// Exchange the stored refresh token of `target`.
    pub async fn refresh_login(&self, target: &LoginTarget) -> login::RefreshResult {
        login::refresh_auth_token_result_in_store(
            &target.host,
            &target.client_id,
            &self.credential_store(),
        )
        .await
    }

    /// The device flow for `target`; the old login stays until the
    /// replacement has been authenticated and saved.
    pub async fn device_login(
        &self,
        target: &LoginTarget,
    ) -> Result<StoredCredentials, ProviderError> {
        login::login_device_flow_in_store(
            &login::LoginEndpoints::from_host(&target.host),
            &target.client_id,
            &tokio_util::sync::CancellationToken::new(),
            &self.credential_store(),
        )
        .await
    }

    /// Remove the stored credential of `host` (and its OS-store copy).
    pub fn delete_github_credential(&self, host: &str) -> Result<(), ProviderError> {
        self.credential_store().delete(host)
    }
}
