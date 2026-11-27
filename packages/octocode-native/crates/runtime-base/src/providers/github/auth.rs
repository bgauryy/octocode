mod credential_store;
mod discovery;
mod home_store;
mod resolver;
mod storage;
pub use credential_store::CredentialStore;
pub use resolver::{AuthMode, AuthSelection, Authentication, CallSelection};
pub use storage::{OAuthToken, StoredCredentials};

/// The credential host a configured GitHub API URL authenticates against
/// ([`octocode_github::credential_host`]); every surface (tool requests, CLI
/// auth status, login, logout) keys credentials by it. Returns `None` when
/// the URL cannot be parsed or has no host: the env token is then not
/// attached, and CLI auth commands fall back to `github.com`.
pub fn configured_credential_host(api_url: &str) -> Option<String> {
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
mod tests;
