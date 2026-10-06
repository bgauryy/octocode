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

pub use octocode_github::{
    CredentialRequest, CredentialResolver, CredentialSource, ResolvedCredential,
    StaticCredentialResolver,
};

/// The credential host a configured GitHub API URL authenticates against
/// ([`octocode_github::credential_host`]). Returns `None` when the URL cannot
/// be parsed or has no host, in which case the env token is not attached.
fn configured_credential_host(api_url: &str) -> Option<String> {
    let url = url::Url::parse(api_url).ok()?;
    Some(octocode_github::credential_host(url.host_str()?).to_ascii_lowercase())
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
