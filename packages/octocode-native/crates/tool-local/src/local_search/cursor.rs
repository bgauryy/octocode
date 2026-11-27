//! Continuations and snapshot identity: the next-page calls a result
//! names and the fingerprint that proves a cursor still answers its query.

use super::local_search_query;
use octocode_contract_types::{ErrorCode, LocalSearchQuery};
use octocode_engine::digest::{Nulls, canonical_json};
use octocode_engine::num::saturating_u32;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::prune::DefaultsFlag;
use octocode_runtime_base::response::result::Continuation;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::response::result::query_value;
use serde_json::{Value, json};

/// The page's continuation: the next file page of a kept scan. Storing the
/// scan for later pages happens here, once a page names the snapshot.
pub(super) fn cursor(
    query: &LocalSearchQuery,
    view: &super::page_view::PageView,
    result_identity: &str,
    reusable: Option<super::manifest::Fresh>,
    policy_key: String,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<Value>, ToolError> {
    let page = local_search_query::page(query).max(1);
    let layout = &view.layout;
    let empty = view.totals.files == 0;
    let snapshot = (!empty
        && (local_search_query::snapshot(query).is_some() || page < layout.total_pages))
        .then(|| result_identity.to_owned());
    if snapshot.is_some()
        && let Some(scan) = reusable
    {
        super::manifest::put_checked(result_identity.to_owned(), policy_key, scan, cancelled);
    }
    build_next(query, page, layout.total_pages, snapshot.as_deref())
}

/// The call that continues a streamed page where its walk goes on
/// ([`super::executor::streamed`]); `None` when the walk is over. `listed`
/// counts the rows (files in list views) the pages so far showed.
pub(super) fn walk_next(
    build: &str,
    query: &LocalSearchQuery,
    cut: Option<&super::executor::streamed::WalkCut>,
    listed: usize,
) -> Result<Option<Value>, ToolError> {
    let Some(cut) = cut else {
        return Ok(None);
    };
    // A zero cursor position is its omission.
    let position = |value: usize| (value > 0).then(|| json!(value));
    let changes = json!({
        "walkOffset": position(cut.walk as usize),
        "rowOffset": position(cut.row),
        "scanOffset": position(listed),
        "snapshot": super::executor::streamed::snapshot_of(
            build,
            &cut.pin,
            &cut.anchor,
            &cut.carried,
        ),
    });
    let next = local_search_query::edited(query, &changes)?;
    Ok(Some(
        octocode_runtime_base::response::collection::walk_calls(
            ToolId::LocalSearch,
            None,
            Continuation::continued_query(next),
        ),
    ))
}

/// A continuation whose snapshot no longer describes the source; `next.restart`
/// reruns the query from its first page, without any cursor.
pub fn stale_snapshot(query: &LocalSearchQuery) -> ToolError {
    ToolError::stale_snapshot(octocode_runtime_base::response::result::restart_query(
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
    snapshot: Option<&str>,
) -> Result<Option<Value>, ToolError> {
    let mut map = json!({});
    let mut base = query_value(q);
    if let Some(snapshot) = snapshot {
        base["snapshot"] = json!(snapshot);
    }
    if page < total_pages
        && let Some(call) = octocode_runtime_base::response::collection::continued_page(
            ToolId::LocalSearch,
            base,
            "page",
            page as usize + 1,
            &[],
        )
    {
        octocode_runtime_base::response::collection::insert_walk(
            &mut map,
            ToolId::LocalSearch,
            None,
            call,
        );
    }
    Ok((map.as_object().is_some_and(|calls| !calls.is_empty())).then_some(map))
}

/// Snapshot identity: the [`query_key`] (the validated root plus every
/// field that changes which matches are collected or how their values read,
/// with defaults applied), then the collected result and source digests.
/// Source outside matched lines can change enclosing names and page cuts.
/// Pagination fields (`page`, `pageSize`) only select
/// from that result and stay out, so a continuation may change them while
/// keeping its snapshot.
///
/// The result is hashed as it is encoded ([`super::packed::Identity`]), so
/// proving a cursor costs one pass over the rows and keeps no copy of them.
pub(super) fn fingerprint(
    build: &str,
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
            stats.files_matched.unwrap_or(saturating_u32(files.len())),
        ))
        .optional(Some(stats.files_searched.unwrap_or(0)))
        .number(u64::from(stats.capped.unwrap_or(false)))
        .optional_text(cap_text(&stats.cap_reasons).as_deref())
        .optional(stats.error_count.filter(|count| *count > 0))
        .optional_text(stats.first_error.as_deref());
    // The digest binds the source and complete value identities that
    // determine page cuts and duplicate ranking, including text outside
    // visible rows, hashed from a byte encoding. The build identity before it
    // retires every cursor another build minted, so no version is bumped by
    // hand when the page cut changes.
    format!("{SNAPSHOT_SCHEME}{}:{}", build, identity.finish())
}

pub(super) const SNAPSHOT_SCHEME: &str = "lexical-live-";

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

/// A `snapshot` no search could have issued: not `lexical-live-<build>:` plus a
/// SHA-256, and for a walk's continuation `:` and where the page before began
/// ([`octocode_engine::search::text_search::Anchor`]). That is input to
/// correct; a well-formed snapshot of another build or another result is
/// stale and restarts instead.
pub(super) fn malformed_snapshot(query: &LocalSearchQuery) -> Option<ToolError> {
    let snapshot = local_search_query::snapshot(query)?;
    let well_formed = snapshot
        .strip_prefix(SNAPSHOT_SCHEME)
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(build, pinned)| {
            let (digest, anchor) = pinned
                .split_once(':')
                .map_or((pinned, None), |(digest, anchor)| (digest, Some(anchor)));
            !build.is_empty()
                && build.bytes().all(|byte| byte.is_ascii_hexdigit())
                && digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
                && anchor
                    .is_none_or(|anchor| super::executor::streamed::decode_anchor(anchor).is_some())
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
const NON_SEMANTIC_FIELDS: [&str; 5] =
    ["page", "scanOffset", "walkOffset", "rowOffset", "snapshot"];

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
    // A field stated at its contract default is its omission, as a
    // continuation that drops it runs.
    octocode_runtime_base::response::result::canonical_row(
        octocode_runtime_base::id::ToolId::LocalSearch,
        &mut semantics,
    );
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
    octocode_runtime_base::digest::json_sha256(&canonical_json(
        json!([root.to_string_lossy(), semantics]),
        Nulls::Drop,
    ))
}

#[cfg(test)]
mod tests;
