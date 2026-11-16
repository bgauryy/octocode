//! The git URLs of a GitHub endpoint: where repositories are fetched from.
use super::GitError;
use octocode_contract_types::ErrorCode;
use octocode_github::{Deployment, GitHubEndpoint};

/// The git web root of `endpoint` that repositories are cloned from, by its
/// deployment: GitHub's cloud serves its API at the root of `api.<web host>`
/// and git at the web host (`https://github.com`, `https://<sub>.ghe.com`);
/// an Enterprise Server serves git at its host and any path prefix before
/// `/api/v3`.
pub(crate) fn web_root(endpoint: &GitHubEndpoint) -> Result<url::Url, GitError> {
    let base = endpoint
        .rest(&[])
        .map_err(|error| GitError::new(ErrorCode::Config, error.message))?;
    let secure = base.scheme() == "https"
        || (base.scheme() == "http" && octocode_github::is_loopback(&base));
    if !secure
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(unsupported_endpoint());
    }
    let path = base.path().trim_end_matches('/');
    if endpoint.deployment() == Deployment::Cloud {
        let host = base.host_str().unwrap_or_default();
        let web = octocode_github::credential_host(host);
        // A cloud API lives at the root of its `api.` host only, on the
        // default port: the git host has no other port to carry over.
        if !path.is_empty() || web == host || base.port().is_some() {
            return Err(unsupported_endpoint());
        }
        return url::Url::parse(&format!("https://{web}")).map_err(|_| unsupported_endpoint());
    }
    let Some(prefix) = path.strip_suffix("/api/v3") else {
        return Err(unsupported_endpoint());
    };
    let mut url = base.clone();
    url.set_path(prefix);
    Ok(url)
}

/// The git URL of `owner/repo` under `root` ([`web_root`]).
pub(crate) fn repository_url(root: &url::Url, owner: &str, repo: &str) -> Result<String, GitError> {
    let mut url = root.clone();
    url.path_segments_mut()
        .map_err(|_| unsupported_endpoint())?
        .pop_if_empty()
        .push(owner)
        .push(&format!("{repo}.git"));
    Ok(url.into())
}

fn unsupported_endpoint() -> GitError {
    GitError::new(
        ErrorCode::Config,
        "ghCloneRepo requires an HTTPS GitHub API endpoint (plain http only on loopback): https://api.github.com, https://api.<subdomain>.ghe.com or a GitHub Enterprise Server endpoint ending in /api/v3, without credentials or query parameters. Use ghGetFileContent for other API proxies.",
    )
}
