//! Go discovery paging: pkg.go.dev search pages, bound cursors and token walks.
use super::super::http::{JSON, RegistryClient};
use super::super::util::{endpoint, object_for, string};
use super::super::{ArtifactError, ArtifactSearchQuery, ArtifactType};
use crate::providers::artifact::types::artifact_search_query;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// pkg.go.dev serves at most this many search results and reports this
/// number as `total` for any broader search.
pub(in crate::providers::artifact) const GO_SEARCH_CAP: u64 = 1_000;

/// The identity of one Go discovery page's start: its terms, page size and
/// page. The cache remembers each start's token under it, and a cursor is
/// bound to it.
pub(super) fn go_page_key(query: &ArtifactSearchQuery, page: u64) -> String {
    format!(
        "go:search:{}:{}:{page}",
        artifact_search_query::page_size(query),
        artifact_search_query::terms(query)
    )
}

/// A pkg.go.dev token as the wire cursor of the page `key` names: the token
/// behind a digest of that page's identity, so a cursor copied onto another
/// query or page is ignored (that page walks) instead of serving rows that
/// are not its own.
pub(super) fn go_cursor(key: &str, token: &str) -> String {
    format!("{}.{token}", go_cursor_binding(key))
}

/// The token of a cursor bound to the page `key` names; `None` for a cursor
/// bound to any other page.
pub(super) fn go_token<'a>(key: &str, cursor: &'a str) -> Option<&'a str> {
    cursor
        .strip_prefix(go_cursor_binding(key).as_str())?
        .strip_prefix('.')
        .filter(|token| !token.is_empty())
}

fn go_cursor_binding(key: &str) -> String {
    // The first 8 bytes of the digest, as hex.
    octocode_engine::digest::sha256(key.as_bytes())[..16].to_owned()
}

/// One pkg.go.dev search page: the first, or the one `token` starts.
pub(super) async fn go_search(
    query: &ArtifactSearchQuery,
    token: Option<String>,
    client: &RegistryClient<'_>,
) -> Result<serde_json::Map<String, Value>, ArtifactError> {
    let url = endpoint(
        "https://pkg.go.dev/v1/search",
        &[
            ("q", Some(artifact_search_query::terms(query))),
            (
                "limit",
                Some(artifact_search_query::page_size(query).to_string()),
            ),
            ("token", token),
        ],
    )?;
    let response = client
        .json(ArtifactType::Go, url, false, None, None, JSON)
        .await?
        .ok_or_else(|| super::super::util::invalid(ArtifactType::Go))?;
    Ok(object_for(&response, ArtifactType::Go)?.clone())
}

/// Search page `page` reached by following tokens from the deepest page
/// whose start the cache remembers (else page 1), remembering each start
/// on the way, and whether it was reached: the results can end before it
/// (then the last page read, for its total).
pub(super) async fn go_walk(
    query: &ArtifactSearchQuery,
    page: u64,
    client: &RegistryClient<'_>,
) -> Result<(serde_json::Map<String, Value>, bool), ArtifactError> {
    let (mut hop, mut token) = (2..=page)
        .rev()
        .find_map(|hop| Some((hop, Some(client.cursor(&go_page_key(query, hop))?))))
        .unwrap_or((1, None));
    let walked = hop < page;
    loop {
        let data = go_search(query, token, client).await.map_err(|mut error| {
            // A deadline mid-walk is the walk's length, not the registry's
            // availability: name the walk and the one-request way to the page.
            if walked && error.code == ErrorCode::Timeout {
                error.message = format!(
                    "Go search pages are walked one registry token at a time; the walk stopped at page {hop} of {page}."
                );
                error.hints = vec![
                    "Follow an earlier page's nextPage (one request each), or retry: walked pages persist unless storage is in memory mode.".into(),
                ];
            }
            error
        })?;
        if hop >= page {
            return Ok((data, true));
        }
        let Some(next) = string(data.get("nextPageToken")) else {
            return Ok((data, false));
        };
        client.remember_cursor(&go_page_key(query, hop + 1), &next);
        token = Some(next);
        hop += 1;
    }
}
