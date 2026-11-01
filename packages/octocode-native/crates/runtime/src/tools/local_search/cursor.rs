//! Continuations and snapshot identity: the next-page calls a result
//! names and the fingerprint that proves a cursor still answers its query.

use super::layout::*;
use super::local_search_query;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::Continuation;
use crate::response::result::ToolError;
use crate::response::result::query_value;
use octocode_contract_types::{ErrorCode, LocalSearchQuery};
use octocode_engine::digest::{Nulls, canonical_json};
use serde_json::{Value, json};

/// The page's continuations, and whether rows remain in its own files.
/// Storing the scan for later pages happens here, once a page names the
/// snapshot.
pub(super) fn cursor(
    query: &LocalSearchQuery,
    parsed: &octocode_engine::types::TextSearchResult,
    layout: &Layout,
    result_identity: &str,
    reusable: Option<super::manifest::Fresh>,
    policy_key: String,
    selection: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> (Option<Value>, bool) {
    let page = local_search_query::page(query).max(1);
    let match_page = local_search_query::match_page(query).max(1);
    let streamed = layout.streamed;
    let empty = parsed.files.is_empty();
    // A streamed page names its continuation; a grid page also needs the
    // snapshot while a shown file has rows beyond one match page.
    let snapshot = (selection.is_some()
        || (!empty
            && (local_search_query::snapshot(query).is_some()
                || page < layout.total_pages
                || (!streamed
                    && parsed
                        .files
                        .iter()
                        .any(|f| u32_of_usize(f.matches.len()) > layout.matches_per)))))
    .then(|| result_identity.to_owned());
    if snapshot.is_some()
        && let Some(scan) = reusable
    {
        super::manifest::put_checked(result_identity.to_owned(), policy_key, scan, cancelled);
    }
    // Leftover rows only count on the files this page shows: another page's
    // files are reached by `nextPage` (which restarts at matchPage 1). List
    // views emit no match rows, so they have none left to page, and a
    // streamed page's later rows are on its later pages.
    let leftover_matches = !layout.list
        && !streamed
        && layout.shown.iter().any(|(index, _)| {
            u32_of_usize(parsed.files[*index].matches.len())
                > match_page.saturating_mul(layout.matches_per)
        });
    let next = build_next(
        query,
        page,
        layout.total_pages,
        leftover_matches,
        match_page,
        snapshot.as_deref(),
        selection,
    );
    (next, leftover_matches)
}

/// A continuation whose snapshot no longer describes the source; `next.restart`
/// reruns the query from its first page, without any cursor.
pub(super) fn stale_snapshot(query: &LocalSearchQuery) -> ToolError {
    ToolError::stale_snapshot(crate::response::result::restart_query(
        ToolId::LocalSearch,
        &query_value(query),
    ))
}

/// A stored scan's file no longer hashes to the bytes its values came from:
/// drop the scan and restart rather than show or check them against new text.
pub(super) fn changed_since_scan(query: &LocalSearchQuery) -> ToolError {
    if let Some(snapshot) = local_search_query::snapshot(query) {
        super::manifest::evict(snapshot);
    }
    stale_snapshot(query)
}

pub(super) fn build_next(
    q: &LocalSearchQuery,
    page: u32,
    total_pages: u32,
    leftover_matches: bool,
    match_page: u32,
    snapshot: Option<&str>,
    selection: Option<&str>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();
    let mut base = query_value(q);
    if let Some(token) = selection {
        base["selection"] = json!(token);
    }
    if let Some(snapshot) = snapshot {
        base["snapshot"] = json!(snapshot);
    }
    let next_match_page = leftover_matches.then(|| match_page as usize + 1);
    // Rows left in the shown files (`nextMatchPage`) and later files
    // (`nextPage`) are both offered on the page that opens a file group
    // (matchPage 1), so the caller goes depth-first or breadth-first. A
    // middle match page of the same group does not offer `nextPage` again;
    // the page that ends the group does, so a page with later files always
    // names a continuation and the depth-first route reaches them. That
    // repeats one idempotent call (the same snapshot), at most once per
    // group. A chain's first call (no snapshot yet) offers it at any match
    // page, since no earlier page did.
    let opens_file_group = match_page <= 1 || local_search_query::snapshot(q).is_none();
    if page < total_pages
        && (opens_file_group || next_match_page.is_none())
        && let Some(call) = crate::response::paging::next_page_call(
            ToolId::LocalSearch,
            base.clone(),
            "page",
            page as usize + 1,
            &[("matchPage", json!(1))],
        )
    {
        map.insert("nextPage".into(), call);
    }
    if let Some(next_match_page) = next_match_page {
        let mut n = base;
        n["matchPage"] = json!(next_match_page);
        map.insert(
            "nextMatchPage".into(),
            Continuation::continued(ToolId::LocalSearch, n).build(),
        );
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// Snapshot identity: the [`query_key`] (the validated root plus every
/// field that changes which matches are collected or how their values read,
/// with defaults applied), then the collected result and source digests.
/// Source outside matched lines can change enclosing names and page cuts.
/// Pagination fields (`page`, `matchPage`, `pageSize`, `matchPageSize`) only select
/// from that result and stay out, so a continuation may change them while
/// keeping its snapshot.
///
/// The result is hashed as it is encoded ([`super::packed::Identity`]), so
/// proving a cursor costs one pass over the rows and keeps no copy of them.
pub(super) fn fingerprint(
    query_key: &str,
    files: &[octocode_engine::types::TextSearchFile],
    stats: &octocode_engine::types::TextSearchStats,
) -> String {
    let mut identity = super::packed::Identity::default();
    identity.text(query_key).number(files.len() as u64);
    for file in files {
        identity.file(file);
    }
    identity
        .optional(Some(stats.match_count.unwrap_or(0)))
        .optional(Some(stats.matched_lines.unwrap_or(0)))
        .optional(Some(
            stats.files_matched.unwrap_or(u32_of_usize(files.len())),
        ))
        .optional(Some(stats.files_searched.unwrap_or(0)))
        .number(u64::from(stats.capped.unwrap_or(false)))
        .optional_text(cap_text(&stats.cap_reasons).as_deref())
        .optional(stats.error_count.filter(|count| *count > 0))
        .optional_text(stats.first_error.as_deref());
    // v6 binds what v5 bound (the source and complete value identities that
    // determine page cuts and duplicate ranking, including text outside
    // visible rows), hashed from a byte encoding instead of JSON. v8 binds
    // what v7 bound; its pages charge exact names for the files that decide
    // the first page instead of a fixed first 64 files, so a v7 cursor would
    // resume at another cut and restarts instead.
    format!("{SNAPSHOT_SCHEME}{SNAPSHOT_VERSION}:{}", identity.finish())
}

const SNAPSHOT_SCHEME: &str = "lexical-live-v";
const SNAPSHOT_VERSION: u32 = 8;

/// The cap reasons as one stable text, as the snapshot fingerprint binds
/// them: comma-joined names in their fixed order, `None` when uncapped.
fn cap_text(reasons: &[octocode_engine::types::CapReason]) -> Option<String> {
    (!reasons.is_empty()).then(|| {
        reasons
            .iter()
            .map(|reason| reason.name())
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// A `snapshot` no search could have issued: not `lexical-live-v<N>:` plus a
/// SHA-256. That is input to correct; a well-formed snapshot of an older
/// version or another result is stale and restarts instead.
pub(super) fn malformed_snapshot(query: &LocalSearchQuery) -> Option<ToolError> {
    let snapshot = local_search_query::snapshot(query)?;
    let well_formed = snapshot
        .strip_prefix(SNAPSHOT_SCHEME)
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(version, digest)| {
            !version.is_empty()
                && version.bytes().all(|byte| byte.is_ascii_digit())
                && digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        });
    (!well_formed).then(|| {
        ToolError::new(
            ErrorCode::InvalidInput,
            "snapshot is not one a localSearch result issued; drop `snapshot` to search again",
        )
    })
}

/// Fields that only pick a page from a collected result, name a cursor or
/// carry the caller's intent: they stay out of [`query_key`], so a
/// continuation may change them and keep its snapshot. Every other field
/// is search semantics, including any field the contract adds later.
const NON_SEMANTIC_FIELDS: [&str; 5] = [
    "page",
    "matchPage",
    "snapshot",
    "selection",
    "classification",
];

/// Digest of the search semantics a snapshot answers: the whole normalized
/// query less [`NON_SEMANTIC_FIELDS`], with the defaults the walk applies
/// spelled out, under the validated `root` (which replaces `path`), and the
/// page cut.
pub(super) fn query_key(
    q: &LocalSearchQuery,
    root: &std::path::Path,
    page_budget: usize,
) -> String {
    let mut semantics = query_value(q);
    if let Some(fields) = semantics.as_object_mut() {
        for field in NON_SEMANTIC_FIELDS {
            fields.remove(field);
        }
        fields.remove("path");
        fields.insert(
            "defaultExcludes".into(),
            json!(local_search_query::default_excludes(q).defaults()),
        );
        // The page cut: a continuation served under another response window
        // would cut other pages, so it restarts instead of skipping rows.
        fields.insert("pageChars".into(), json!(page_budget));
        // Match-only spans include columns, which affect the page cut.
        fields.insert(
            "spanColumns".into(),
            json!(super::local_search_query::shows_span_columns(q)),
        );
    }
    crate::digest::json_sha256(&canonical_json(
        json!([root.to_string_lossy(), semantics]),
        Nulls::Drop,
    ))
}

#[cfg(test)]
mod tests;
