use super::http::{JSON, RegistryClient};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, TerminalLimit,
};
use crate::providers::artifact::types::artifact_search_query;
use crate::providers::artifact::types::artifact_type;
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value};
use url::Url;

pub(crate) fn object_for(
    value: &Value,
    artifact_type: ArtifactType,
) -> Result<&Map<String, Value>, ArtifactError> {
    value.as_object().ok_or_else(|| invalid(artifact_type))
}

pub(crate) fn rows(
    value: &Value,
    artifact_type: ArtifactType,
) -> Result<&Vec<Value>, ArtifactError> {
    value.as_array().ok_or_else(|| invalid(artifact_type))
}

pub(crate) fn string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// A registry-recorded git commit (7–40 hex digits), lowercased.
pub(crate) fn commit_sha(value: Option<&Value>) -> Option<String> {
    string(value)
        .filter(|sha| (7..=40).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|sha| sha.to_ascii_lowercase())
}

pub(crate) fn required(
    value: Option<&Value>,
    artifact_type: ArtifactType,
) -> Result<String, ArtifactError> {
    string(value).ok_or_else(|| invalid(artifact_type))
}

pub(crate) fn total(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str()?.parse::<u64>().ok())
    })
}

pub(crate) fn safe_url(value: Option<&Value>) -> Option<String> {
    let value = string(value)?;
    let cleaned = value.strip_prefix("git+").unwrap_or(&value);
    let url = Url::parse(cleaned).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.into())
}

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
    let text: String = url.into();
    let text = text.trim_end_matches('/');
    let text = text.strip_suffix(".git").unwrap_or(text);
    Some(text.trim_end_matches('/').to_owned())
}

/// `https://github.com/<owner>/<repo>` of a link into a GitHub repository
/// (a homepage or module path that names one), the repository a registry
/// row without its own `repository` still points at.
pub(crate) fn github_repository(link: &str) -> Option<String> {
    github_slug(link).map(|(owner, repo)| format!("https://github.com/{owner}/{repo}"))
}

pub(crate) fn license(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" OR ");
            (!joined.is_empty()).then_some(joined)
        }
        value => string(Some(value)),
    }
}

pub(crate) fn endpoint(
    base: &str,
    params: &[(&str, Option<String>)],
) -> Result<Url, ArtifactError> {
    let mut url = parse_url(base)?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in params {
            if let Some(value) = value {
                pairs.append_pair(key, value);
            }
        }
    }
    Ok(url)
}

pub(crate) fn parse_url(value: &str) -> Result<Url, ArtifactError> {
    Url::parse(value).map_err(|_| {
        ArtifactError::new(
            ErrorCode::ProviderError,
            "Artifact registry endpoint could not be constructed.",
        )
    })
}

pub(crate) fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(char::from(*byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(crate) fn coordinate_path(value: &str) -> String {
    value
        .split('/')
        .map(encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn invalid(artifact_type: ArtifactType) -> ArtifactError {
    ArtifactError::new(
        ErrorCode::ProviderError,
        format!(
            "{} returned an invalid registry response.",
            artifact_type::as_str(artifact_type)
        ),
    )
}

/// The `YYYY-MM-DD` prefix of an RFC 3339 / ISO 8601 timestamp.
pub(crate) fn date_prefix(value: Option<&Value>) -> Option<String> {
    let value = string(value)?;
    let date = value.get(..10)?;
    (date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-').then(|| date.to_owned())
}

/// `(owner, repo)` of a link into a GitHub repository, in any form
/// [`canonical_repository`] reads (`https://github.com/o/r`,
/// `git+https://github.com/o/r.git`, `git@github.com:o/r`, `github:o/r`,
/// `github.com/o/r/tree/…`, a file or tree page).
/// `None` for another host and for an account-level page such as
/// `github.com/sponsors/<user>`.
pub(crate) fn github_slug(link: &str) -> Option<(String, String)> {
    let url = Url::parse(&canonical_repository(link)?).ok()?;
    if !url
        .host_str()
        .is_some_and(|host| matches!(host, "github.com" | "www.github.com"))
    {
        return None;
    }
    let mut segments = url.path_segments()?.filter(|part| !part.is_empty());
    let owner = segments.next()?;
    let repo = segments.next()?.trim_end_matches(".git");
    (!repo.is_empty() && !matches!(owner, "sponsors" | "orgs" | "users" | "apps"))
        .then(|| (owner.to_owned(), repo.to_owned()))
}

/// Monorepo subdirectory from a `/tree/<ref>/<dir>` or `/blob/<ref>/<file>`
/// repository URL. The ref is dropped: refs may contain slashes, and URL refs
/// are often stale; only viewReleaseSource pins the registry's source ref.
pub(crate) fn github_repo_dir(url: &str) -> Option<String> {
    let (kind, _ref, parts) = github_page(url)?;
    let mut segments: Vec<&str> = parts.collect();
    match kind {
        "tree" => {}
        "blob" => {
            segments.pop();
        }
        _ => return None,
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

/// The ref of a `/tree/<ref>` or `/blob/<ref>` repository URL when it names
/// `version`'s release (`v8.1.4` for 8.1.4) and nothing after it: a branch
/// or a monorepo directory URL is not release evidence.
pub(crate) fn release_ref_in(url: &str, version: &str) -> Option<String> {
    let (kind, reference, mut parts) = github_page(url)?;
    let tagged = !version.is_empty()
        && reference
            .strip_suffix(version)
            .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with(['v', '-', '/', '@']));
    (matches!(kind, "tree" | "blob") && parts.next().is_none() && tagged)
        .then(|| reference.to_owned())
}

/// The `<kind>` and `<ref>` of a `github.com/<owner>/<repo>/<kind>/<ref>/…`
/// URL, and the path segments after them; the query and fragment are dropped.
fn github_page(url: &str) -> Option<(&str, &str, impl Iterator<Item = &str>)> {
    let rest = url.split_once("github.com/")?.1;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let (_owner, _repo) = (parts.next()?, parts.next()?);
    Some((parts.next()?, parts.next()?, parts))
}

/// A `q`/`page`/`per_page` keyword search URL on `base`.
pub(crate) fn keyword_url(
    base: &str,
    query: &ArtifactSearchQuery,
    page: u64,
    size: usize,
) -> Result<url::Url, ArtifactError> {
    endpoint(
        base,
        &[
            ("q", Some(artifact_search_query::terms(query))),
            ("page", Some(page.to_string())),
            ("per_page", Some(size.to_string())),
        ],
    )
}

/// A keyword search response object and its rows under `key`, each mapped
/// by `item`.
pub(crate) async fn keyword_rows(
    client: &RegistryClient<'_>,
    artifact_type: ArtifactType,
    url: url::Url,
    key: &str,
    item: fn(&serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError>,
) -> Result<(serde_json::Map<String, Value>, Vec<ArtifactItem>), ArtifactError> {
    let invalid = || invalid(artifact_type);
    let Some(Value::Object(data)) = client
        .json(artifact_type, url, false, None, None, JSON)
        .await?
    else {
        return Err(invalid());
    };
    let artifacts = rows(data.get(key).ok_or_else(invalid)?, artifact_type)?
        .iter()
        .map(|row| item(object_for(row, artifact_type)?))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((data, artifacts))
}

pub(crate) fn paged(
    artifacts: Vec<ArtifactItem>,
    total: Option<u64>,
    page: u64,
    size: usize,
    artifact_type: ArtifactType,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let more = total
        .map(|count| page.saturating_mul(size as u64) < count)
        .unwrap_or(artifacts.len() == size);
    let terminal_limit = (more && artifacts.is_empty()).then(|| {
        TerminalLimit::short_page(if artifact_type == ArtifactType::Crates {
            "crates.io"
        } else {
            artifact_type::as_str(artifact_type)
        })
    });
    Ok(ArtifactProviderPage {
        next_state: (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total,
        terminal_limit,
        registry: None,
    })
}

#[cfg(test)]
mod github_page_tests {
    use super::{github_repo_dir, release_ref_in};

    /// Both readers of a `/tree/<ref>` or `/blob/<ref>` URL drop its query
    /// and fragment and read nothing from another page kind.
    #[test]
    fn tree_and_blob_urls_name_a_directory_or_a_release_ref() {
        let dir = |url| github_repo_dir(url);
        assert_eq!(
            dir("https://github.com/o/r/tree/main/a/b?x=1").as_deref(),
            Some("a/b")
        );
        assert_eq!(
            dir("https://github.com/o/r/blob/main/a/README.md#top").as_deref(),
            Some("a")
        );
        assert_eq!(dir("https://github.com/o/r/blob/main/README.md"), None);
        assert_eq!(dir("https://github.com/o/r/issues/1/x"), None);
        assert_eq!(dir("https://github.com/o/r"), None);
        let release = |url| release_ref_in(url, "1.2.0");
        assert_eq!(
            release("https://github.com/o/r/tree/v1.2.0#x").as_deref(),
            Some("v1.2.0")
        );
        assert_eq!(
            release("https://github.com/o/r/blob/pkg@1.2.0").as_deref(),
            Some("pkg@1.2.0")
        );
        assert_eq!(release("https://github.com/o/r/tree/v1.2.0/docs"), None);
        assert_eq!(release("https://github.com/o/r/tree/main"), None);
        assert_eq!(release("https://github.com/o/r/releases/v1.2.0"), None);
    }
}

#[cfg(test)]
mod github_slug_tests {
    use super::github_slug;

    #[test]
    fn github_slugs_come_only_from_repository_links() {
        let slug = |owner: &str, repo: &str| Some((owner.to_owned(), repo.to_owned()));
        for (link, expected) in [
            ("https://github.com/o/r", slug("o", "r")),
            ("git+https://github.com/o/r.git", slug("o", "r")),
            ("git@github.com:o/r.git", slug("o", "r")),
            ("ssh://git@github.com/o/r.git", slug("o", "r")),
            (
                "https://github.com/o/r/tree/main/packages/x",
                slug("o", "r"),
            ),
            ("https://github.com/o/r.git/tree/main", slug("o", "r")),
            ("github.com/o/r#readme", slug("o", "r")),
            ("https://www.GitHub.com/o/r?tab=readme", slug("o", "r")),
            (
                "https://github.com/Textualize/rich/blob/master/CHANGELOG.md",
                slug("Textualize", "rich"),
            ),
            ("https://github.com/sponsors/hynek", None),
            ("https://github.com/orgs/o/r", None),
            ("https://github.com/hynek", None),
            ("https://github.com/o", None),
            ("https://gitlab.com/o/r", None),
            ("https://gist.github.com/o/r", None),
            ("https://notgithub.com/o/r", None),
        ] {
            assert_eq!(github_slug(link), expected, "{link}");
        }
    }
}

#[cfg(test)]
mod repository_tests {
    use super::{canonical_repository, github_slug};

    /// Every registry link form normalizes to one canonical URL, and that
    /// URL names the same owner/repo the GitHub tools take.
    #[test]
    fn every_repository_link_form_is_one_canonical_url() {
        for (input, expected) in [
            (
                "github:octokit/rest.js",
                Some("https://github.com/octokit/rest.js"),
            ),
            (
                "git+https://github.com/a/b.git",
                Some("https://github.com/a/b"),
            ),
            ("git@github.com:a/b.git", Some("https://github.com/a/b")),
            (
                "ssh://git@github.com/a/b.git",
                Some("https://github.com/a/b"),
            ),
            (
                "ssh://git@github.com:22/a/b",
                Some("https://github.com/a/b"),
            ),
            ("git://github.com/a/b.git", Some("https://github.com/a/b")),
            (
                "scm:git:git@github.com:a/b.git",
                Some("https://github.com/a/b"),
            ),
            (
                "scm:git:https://github.com/a/b",
                Some("https://github.com/a/b"),
            ),
            ("https://github.com/a/b/", Some("https://github.com/a/b")),
            (
                "https://github.com/a/b.git/",
                Some("https://github.com/a/b"),
            ),
            ("a/b", Some("https://github.com/a/b")),
            ("github.com/a/b", Some("https://github.com/a/b")),
            (
                "https://github.com/rails/rails/tree/v8.1.4",
                Some("https://github.com/rails/rails/tree/v8.1.4"),
            ),
            ("https://gitlab.com/g/x", Some("https://gitlab.com/g/x")),
            ("https://user:secret@github.com/a/b", None),
            ("ftp://github.com/a/b", None),
            ("", None),
            ("not a link", None),
        ] {
            assert_eq!(canonical_repository(input).as_deref(), expected, "{input}");
            if let Some(expected) = expected.filter(|url| url.contains("github.com/a/b")) {
                assert_eq!(
                    github_slug(expected),
                    Some(("a".to_owned(), "b".to_owned()))
                );
            }
        }
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn dates_from_millis_and_timestamps() {
        assert_eq!(
            crate::civil_date::date_millis(1_789_341_914_484),
            "2026-09-13"
        );
        assert_eq!(
            date_prefix(Some(&Value::from("2019-09-08T01:56:06.955881Z"))).as_deref(),
            Some("2019-09-08")
        );
        assert_eq!(date_prefix(Some(&Value::from("soon"))), None);
    }
}
