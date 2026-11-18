use super::http::{JSON, RegistryClient};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, TerminalLimit,
};
use crate::artifact::query::artifact_search_query;
use crate::artifact::query::artifact_type;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use serde_json::{Map, Value};
use url::Url;

mod repository;

pub use repository::github_repository;
pub(crate) use repository::{canonical_repository, repository_url};

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
pub fn github_slug(link: &str) -> Option<(String, String)> {
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

/// The path segments below `/tree/` or `/blob/` in a GitHub page URL: the ref
/// and, after it, the directory. A blob's last segment is the file, so it is
/// dropped. `None` for any other page, and unless a segment remains below the
/// first one, since a link that stops at its ref names no directory. A ref may
/// itself contain `/`, so the URL alone cannot say where it ends: the caller
/// settles the split ([`super::link_directory`]).
pub(crate) fn github_ref_path(url: &str) -> Option<Vec<&str>> {
    let (kind, reference, parts) = github_page(url)?;
    let mut segments = vec![reference];
    segments.extend(parts);
    match kind {
        "tree" => {}
        "blob" => {
            segments.pop();
        }
        _ => return None,
    }
    (segments.len() > 1).then_some(segments)
}

/// A registry's source link split into the repository it names and, for a
/// GitHub page below a ref, the segments of that page below `tree`/`blob`.
pub(crate) struct SourceLink {
    pub repository: String,
    /// The ref and directory segments of the link ([`github_ref_path`]).
    pub ref_path: Option<Vec<String>>,
}

/// A registry's source link as the repository it names and the page path
/// below its ref. A GitHub link is the repository root
/// (`https://github.com/<owner>/<repo>`) whatever page the link is
/// (`/tree/<ref>/<dir>`, `/issues`); another host keeps its canonical link.
pub(crate) fn source_link(link: &str) -> Option<SourceLink> {
    let canonical = canonical_repository(link)?;
    let ref_path = github_ref_path(&canonical)
        .map(|segments| segments.into_iter().map(str::to_owned).collect());
    let repository = github_repository(&canonical).unwrap_or(canonical);
    Some(SourceLink {
        repository,
        ref_path,
    })
}

/// The ref of a `/tree/<ref>` or `/blob/<ref>` repository URL when it names
/// `version`'s release (`v8.1.4` for 8.1.4) and nothing after it: a branch
/// or a monorepo directory URL is not release evidence.
pub fn release_ref_in(url: &str, version: &str) -> Option<String> {
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
mod github_page_tests;

#[cfg(test)]
mod github_slug_tests;

#[cfg(test)]
mod repository_tests;

#[cfg(test)]
mod date_tests;
