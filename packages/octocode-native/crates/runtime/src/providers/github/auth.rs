mod credential_store;
mod discovery;
mod home_store;
mod resolver;
mod storage;
pub use credential_store::CredentialStore;
pub use resolver::{AuthMode, AuthSelection, Authentication};
pub use storage::{
    OAuthToken, StoredCredentials, delete_platform_credential, load_stored_credentials,
    store_platform_credential, token_from_stored_blob,
};

use std::{future::Future, pin::Pin};

use secrecy::{ExposeSecret, SecretString};

use super::ProviderError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    Override,
    Environment,
    Storage,
    Home,
    GhCli,
}

#[derive(Clone)]
pub struct ResolvedCredential {
    secret: SecretString,
    pub source: CredentialSource,
}

impl ResolvedCredential {
    pub fn new(secret: impl Into<SecretString>, source: CredentialSource) -> Self {
        Self {
            secret: secret.into(),
            source,
        }
    }
    pub(crate) fn expose(&self) -> &str {
        self.secret.expose_secret()
    }
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("secret", &"[REDACTED]")
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Clone)]
pub struct CredentialRequest<'a> {
    pub host: &'a str,
    pub override_token: Option<&'a str>,
}

pub trait CredentialResolver: Send + Sync {
    fn resolve<'a>(
        &'a self,
        request: CredentialRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ResolvedCredential>, ProviderError>> + Send + 'a>>;
}

/// Derive the credential host that a configured GitHub API URL authenticates
/// against, mirroring `GitHubEndpoint::credential_host` (api.github.com maps to
/// github.com). Returns `None` when the URL cannot be parsed or has no host, in
/// which case the env token is not attached.
fn configured_credential_host(api_url: &str) -> Option<String> {
    let url = url::Url::parse(api_url).ok()?;
    let host = url.host_str()?;
    Some(if host.eq_ignore_ascii_case("api.github.com") {
        "github.com".to_owned()
    } else {
        host.to_ascii_lowercase()
    })
}

pub(super) fn normalize_host(host: &str) -> String {
    let lower = host.trim().to_ascii_lowercase();
    lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower)
        .trim_end_matches('/')
        .to_owned()
}
#[derive(Clone, Default)]
pub struct StaticCredentialResolver {
    token: Option<ResolvedCredential>,
}

impl StaticCredentialResolver {
    pub fn anonymous() -> Self {
        Self::default()
    }
    pub fn new(token: impl Into<SecretString>, source: CredentialSource) -> Self {
        Self {
            token: Some(ResolvedCredential::new(token, source)),
        }
    }
}

impl CredentialResolver for StaticCredentialResolver {
    fn resolve<'a>(
        &'a self,
        request: CredentialRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ResolvedCredential>, ProviderError>> + Send + 'a>>
    {
        Box::pin(async move {
            if let Some(token) = request.override_token {
                return Ok(Some(ResolvedCredential::new(
                    token,
                    CredentialSource::Override,
                )));
            }
            Ok(self.token.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{configured_credential_host, token_from_stored_blob};
    use secrecy::ExposeSecret;

    #[test]
    fn configured_credential_host_maps_api_github_to_github_com() {
        assert_eq!(
            configured_credential_host("https://api.github.com").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            configured_credential_host("https://ghe.example.com/api/v3").as_deref(),
            Some("ghe.example.com")
        );
        assert_eq!(configured_credential_host("not a url"), None);
    }

    #[test]
    fn json_blob_yields_inner_token_and_raw_blob_is_unchanged() {
        let json = r#"{"hostname":"github.com","username":"octo","token":{"token":"gho_inner","tokenType":"oauth"},"gitProtocol":"https","createdAt":"t","updatedAt":"t"}"#;
        assert_eq!(token_from_stored_blob(json).expose_secret(), "gho_inner");
        assert_eq!(
            token_from_stored_blob("gho_raw_token").expose_secret(),
            "gho_raw_token"
        );
    }
}
