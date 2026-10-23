//! Query and endpoint validation: repository segments, sparse paths, and the git web root clones come from.
use super::{CloneError, GhCloneRepoQuery, Sparse};
use octocode_contract_types::ErrorCode;
use octocode_github::GitHubEndpoint;

pub(super) fn validate_query(query: &GhCloneRepoQuery) -> Result<(), CloneError> {
    for (name, value) in [
        ("owner", query.owner.as_str()),
        ("repo", query.repo.as_str()),
    ] {
        if value.trim().is_empty()
            || value.contains('/')
            || value.contains('\\')
            || value == "."
            || value == ".."
        {
            return Err(CloneError::new(
                ErrorCode::InvalidInput,
                format!("{name} must be a non-empty GitHub path segment"),
            ));
        }
    }
    if query
        .ref_
        .as_deref()
        .is_some_and(|branch| branch.trim().is_empty())
    {
        return Err(CloneError::new(
            ErrorCode::InvalidInput,
            "ref must not be empty",
        ));
    }
    let sparse = Sparse::of(query);
    if sparse
        .as_ref()
        .is_some_and(|sparse| sparse.paths.is_empty())
    {
        return Err(CloneError::new(
            ErrorCode::InvalidInput,
            "path must name at least one path",
        ));
    }
    for path in sparse.as_ref().map_or(&[][..], Sparse::paths) {
        if !is_sparse_path(path) {
            return Err(CloneError::new(
                ErrorCode::InvalidInput,
                "path must be a non-empty repo-relative path without traversal",
            ));
        }
    }
    Ok(())
}

/// A sparse checkout path that stays inside the repository: not blank, no
/// backslash, no root or drive prefix (`/x`, `C:x`), and no `.` or `..`
/// segment (an interior `a/./b` included, which `Path::components`
/// would silently normalize away). A trailing `/` names a directory.
pub(super) fn is_sparse_path(path: &str) -> bool {
    let drive = path.as_bytes().get(1) == Some(&b':')
        && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    !path.trim().is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && !drive
        && path
            .strip_suffix('/')
            .unwrap_or(path)
            .split('/')
            .all(|segment| !matches!(segment, "." | ".."))
}

/// The git web root of `endpoint` that repositories are cloned from:
/// `https://github.com` for the public API, the GitHub Enterprise host (and
/// any path prefix before `/api/v3`) otherwise.
pub(super) fn web_root(endpoint: &GitHubEndpoint) -> Result<url::Url, CloneError> {
    let base = endpoint
        .rest(&[])
        .map_err(|error| CloneError::new(ErrorCode::Configuration, error.message))?;
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
    if base.host_str() == Some("api.github.com") && path.is_empty() {
        return url::Url::parse("https://github.com").map_err(|_| unsupported_endpoint());
    }
    let Some(prefix) = path.strip_suffix("/api/v3") else {
        return Err(unsupported_endpoint());
    };
    let mut url = base.clone();
    url.set_path(prefix);
    Ok(url)
}

/// The git URL of `owner/repo` under `root` ([`web_root`]).
pub(super) fn repository_url(
    root: &url::Url,
    owner: &str,
    repo: &str,
) -> Result<String, CloneError> {
    let mut url = root.clone();
    url.path_segments_mut()
        .map_err(|_| unsupported_endpoint())?
        .pop_if_empty()
        .push(owner)
        .push(&format!("{repo}.git"));
    Ok(url.into())
}

fn unsupported_endpoint() -> CloneError {
    CloneError::new(
        ErrorCode::Configuration,
        "ghCloneRepo requires an HTTPS GitHub API endpoint (plain http only on loopback): https://api.github.com or a GitHub Enterprise endpoint ending in /api/v3, without credentials or query parameters. Use ghGetFileContent for other API proxies.",
    )
}
