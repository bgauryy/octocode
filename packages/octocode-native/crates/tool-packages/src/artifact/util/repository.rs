//! Registry repository-link normalization.
use super::{github_slug, string};
use serde_json::Value;
use url::Url;

/// The one repository-link normalizer every registry's `repository` passes:
/// the link as a canonical `https://host/path` without `git+`, a trailing
/// `/` or `.git`. It reads `git+https://…`, scp (`git@host:o/r.git`),
/// `ssh://`/`git://`, Maven `scm:git:…`, and npm's `github:`/`gitlab:`/
/// `bitbucket:` and `owner/repo` shorthands. A link with credentials or
/// another scheme is no repository.
pub(crate) fn repository_url(value: Option<&Value>) -> Option<String> {
    canonical_repository(&string(value)?)
}

/// [`repository_url`] of one link text.
pub(crate) fn canonical_repository(link: &str) -> Option<String> {
    let link = link.trim();
    let link = link.strip_prefix("git+").unwrap_or(link);
    // `scm:git:<url>` (Maven SCM): the provider tag before the link.
    let link = link.strip_prefix("scm:").map_or(link, |rest| {
        rest.split_once(':').map_or(rest, |(_, tail)| tail)
    });
    let lower = link.to_ascii_lowercase();
    let shorthand = [
        ("github:", "github.com"),
        ("gitlab:", "gitlab.com"),
        ("bitbucket:", "bitbucket.org"),
    ]
    .into_iter()
    .find(|(prefix, _)| lower.starts_with(prefix));
    let https = if let Some((prefix, host)) = shorthand {
        format!("https://{host}/{}", &link[prefix.len()..])
    } else if let Some((scheme, rest)) = link.split_once("://") {
        match scheme.to_ascii_lowercase().as_str() {
            "http" | "https" => link.to_owned(),
            // `ssh://git@host[:port]/o/r`: the host serves the same
            // repository over HTTPS; user and port are transport details.
            "ssh" | "git" => {
                let rest = rest.split_once('@').map_or(rest, |(_, tail)| tail);
                let (authority, path) = rest.split_once('/')?;
                let host = authority
                    .split_once(':')
                    .map_or(authority, |(host, _)| host);
                format!("https://{host}/{path}")
            }
            _ => return None,
        }
    } else if let Some((user_host, path)) = link.split_once(':')
        && let Some((_, host)) = user_host.split_once('@')
    {
        // scp form `git@host:o/r.git`.
        format!("https://{host}/{path}")
    } else {
        let first = link.split('/').next().unwrap_or_default();
        match link.matches('/').count() {
            // npm `owner/repo` is a GitHub repository.
            1 if !first.contains('.') && !link.contains(':') => {
                format!("https://github.com/{link}")
            }
            // `host.tld/o/r` without a scheme.
            count if count >= 2 && first.contains('.') => format!("https://{link}"),
            _ => return None,
        }
    };
    let url = Url::parse(&https).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none_or(str::is_empty)
    {
        return None;
    }
    if let Some(mirror) = apache_github_mirror(&url) {
        return Some(mirror);
    }
    let text: String = url.into();
    let text = text.trim_end_matches('/');
    let text = text.strip_suffix(".git").unwrap_or(text);
    Some(text.trim_end_matches('/').to_owned())
}

/// The GitHub mirror of an Apache repository on `gitbox.apache.org` (clone
/// path `/repos/asf/<name>.git` or gitweb `?p=<name>.git`): the ASF mirrors
/// every gitbox repository to `github.com/apache/<name>` under the same name,
/// and that mirror is where the GitHub tools read it.
fn apache_github_mirror(url: &Url) -> Option<String> {
    if url.host_str() != Some("gitbox.apache.org") {
        return None;
    }
    let from_path = url
        .path()
        .strip_prefix("/repos/asf/")
        .map(|name| name.trim_end_matches('/').to_owned());
    let from_query = url
        .query_pairs()
        .find(|(key, _)| key == "p")
        .map(|(_, name)| name.split(';').next().unwrap_or_default().to_owned());
    let name = from_path.or(from_query)?;
    let name = name.strip_suffix(".git").unwrap_or(&name);
    (!name.is_empty() && !name.contains('/')).then(|| format!("https://github.com/apache/{name}"))
}

/// `https://github.com/<owner>/<repo>` of a link into a GitHub repository
/// (a homepage or module path that names one), the repository a registry
/// row without its own `repository` still points at.
pub fn github_repository(link: &str) -> Option<String> {
    github_slug(link).map(|(owner, repo)| format!("https://github.com/{owner}/{repo}"))
}
