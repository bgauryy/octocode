//! Continuations and snapshot identity: the next-page calls a result
//! names and the fingerprint that proves a cursor still answers its query.

use super::executor::*;
use super::layout::*;
use super::types::*;
use crate::canonical_json::canonicalize;
use crate::policy::prune::DefaultsFlag;
use crate::tools::id::ToolId;
use crate::tools::id::query_limits::local_search::{MATCH_PAGE_MAXIMUM, PAGE_MAXIMUM};
use crate::tools::result::Continuation;
use serde_json::{Value, json};

/// The page's continuations, and whether rows remain in its own files.
/// Storing the scan for later pages happens here, once a page names the
/// snapshot.
pub(super) fn cursor(
    query: &LocalSearchQuery,
    parsed: &octocode_engine::types::RipgrepParseResult,
    layout: &Layout,
    result_identity: &str,
    reusable: Option<octocode_engine::types::RipgrepParseResult>,
    policy_key: String,
    skipped: &crate::policy::discovery::WalkSkips,
) -> (Option<Value>, bool) {
    let page = query.page().max(1);
    let match_page = query.match_page().max(1);
    let streamed = layout.streamed;
    let empty = parsed.files.is_empty();
    // A streamed page names its continuation; a grid page also needs the
    // snapshot while a shown file has rows beyond one match page.
    let snapshot = (!empty
        && (query.snapshot.is_some()
            || page < layout.total_pages
            || (!streamed
                && parsed
                    .files
                    .iter()
                    .any(|f| f.matches.len() as u32 > layout.matches_per))))
    .then(|| result_identity.to_owned());
    if snapshot.is_some()
        && let Some(scan) = reusable
    {
        super::manifest::put(
            result_identity.to_owned(),
            policy_key,
            scan,
            skipped.clone(),
        );
    }
    // Leftover rows only count on the files this page shows: another page's
    // files are reached by `nextPage` (which restarts at matchPage 1). List
    // views emit no match rows, so they have none left to page, and a
    // streamed page's later rows are on its later pages.
    let leftover_matches = !layout.list
        && !streamed
        && layout.shown.iter().any(|(index, _)| {
            parsed.files[*index].matches.len() as u32
                > match_page.saturating_mul(layout.matches_per)
        });
    let next = build_next(
        query,
        page,
        layout.total_pages,
        leftover_matches,
        match_page,
        snapshot.as_deref(),
        streamed,
    );
    (next, leftover_matches)
}

/// A continuation whose snapshot no longer describes the source; `next.restart`
/// reruns page 1 without it.
pub(super) fn stale_snapshot(query: &LocalSearchQuery) -> LocalSearchError {
    let mut restart = normalized_query(query, streamed_layout(query));
    if let Some(object) = restart.as_object_mut() {
        object.remove("snapshot");
    }
    restart["page"] = json!(1);
    restart["matchPage"] = json!(1);
    LocalSearchError {
        code: "staleSnapshot",
        message: crate::response::pages::STALE_SNAPSHOT_ERROR.into(),
        hints: vec![],
        next: Some(Box::new(json!({
            "restart": Continuation::new(ToolId::LocalSearch, restart)
                .why("Start a new search against the current source.")
                .confidence("exact")
                .build()
        }))),
    }
}

/// A stored scan's file no longer hashes to the bytes its values came from:
/// drop the scan and restart rather than show or check them against new text.
pub(super) fn changed_since_scan(query: &LocalSearchQuery) -> LocalSearchError {
    if let Some(snapshot) = query.snapshot() {
        super::manifest::evict(snapshot);
    }
    stale_snapshot(query)
}

/// The query as continuations carry it. The window fields (`contextLines`,
/// `matchContentLength`) stay as the caller sent them: every call derives
/// their defaults from the view the same way, and so does the snapshot
/// identity. A streamed layout carries no `pageSize`: its pages are cut by
/// the budget.
pub(super) fn normalized_query(q: &LocalSearchQuery, streamed: bool) -> Value {
    let mut value = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    // `LocalSearchQuery` serializes to a JSON object.
    #[allow(clippy::expect_used)]
    let o = value.as_object_mut().expect("request object");
    o.retain(|_, v| !v.is_null());
    o.entry("caseMode").or_insert(json!("smart"));
    let view = q.result_view;
    o.entry("multiline").or_insert(json!("off"));
    o.entry("sort").or_insert(json!("relevance"));
    o.entry("unique").or_insert(json!("off"));
    o.entry("matchPage").or_insert(json!(1));
    o.entry("page").or_insert(json!(1));
    o.entry("resultView").or_insert(json!("paginated"));
    if !streamed {
        o.entry("pageSize")
            .or_insert(json!(default_page_size(view)));
    }
    value
}

pub(super) fn build_next(
    q: &LocalSearchQuery,
    page: u32,
    total_pages: u32,
    leftover_matches: bool,
    match_page: u32,
    snapshot: Option<&str>,
    streamed: bool,
) -> Option<Value> {
    let mut map = serde_json::Map::new();
    let base = normalized_query(q, streamed);
    // A `nextPage` continuation is never emitted past the contract page
    // maximum; callers must narrow the query to reach later results.
    if page < total_pages && (page as usize) < PAGE_MAXIMUM {
        let mut n = base.clone();
        n["page"] = json!(page + 1);
        // A new file page starts at each file's first match row.
        n["matchPage"] = json!(1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextPage".into(),
            Continuation::new(ToolId::LocalSearch, n)
                .confidence("exact")
                .build(),
        );
    }
    if leftover_matches && (match_page as usize) < MATCH_PAGE_MAXIMUM {
        let mut n = base;
        n["matchPage"] = json!(match_page + 1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextMatchPage".into(),
            Continuation::new(ToolId::LocalSearch, n)
                .confidence("exact")
                .build(),
        );
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// Snapshot identity: the validated root plus every field that changes which
/// matches are collected or how their values read, with defaults applied,
/// then the collected result itself. Pagination fields (`page`, `matchPage`,
/// `pageSize`, `matchPageSize`) only select from that result and stay
/// out, so a continuation may change them while keeping its snapshot.
pub(super) fn fingerprint(
    q: &LocalSearchQuery,
    root: &std::path::Path,
    files: &[octocode_engine::types::RipgrepFile],
    stats: &octocode_engine::types::RipgrepStats,
    page_budget: usize,
) -> String {
    let query_key = query_key(q, root, page_budget);
    let file_values = files
        .iter()
        .map(|f| {
            let matches = f
                .matches
                .iter()
                .map(|m| {
                    let mut o = serde_json::Map::new();
                    o.insert("line".into(), json!(m.line));
                    o.insert("column".into(), json!(m.column));
                    o.insert("value".into(), json!(m.value));
                    if let Some(c) = m.count {
                        o.insert("count".into(), json!(c));
                    }
                    Value::Object(o)
                })
                .collect::<Vec<_>>();
            json!({"path":f.path,"matchCount":f.match_count,"matches":matches})
        })
        .collect::<Vec<_>>();
    let canonical = canonicalize(
        json!([query_key,file_values,{"totalMatches":stats.match_count.unwrap_or(0),"totalMatchedLines":stats.matched_lines.unwrap_or(0),"filesMatched":stats.files_matched.unwrap_or(files.len() as u32),"filesScanned":stats.files_searched.unwrap_or(0),"capped":stats.capped.unwrap_or(false),"capReason":stats.cap_reason,"errorCount":stats.error_count.filter(|n|*n>0),"firstError":stats.first_error} ]),
    );
    format!("lexical-live-v1:{}", crate::digest::json_sha256(&canonical))
}

/// Digest of the search semantics a snapshot answers: the query fields
/// that change which rows match or how pages cut, under `root`.
pub(super) fn query_key(
    q: &LocalSearchQuery,
    root: &std::path::Path,
    page_budget: usize,
) -> String {
    let mut identity = serde_json::Map::new();
    // The page cut: a continuation served under another response window
    // would cut other pages, so it restarts instead of skipping rows.
    identity.insert("pageChars".into(), json!(page_budget));
    identity.insert(
        "defaultExcludes".into(),
        json!(q.default_excludes.defaults()),
    );
    identity.insert("matchString".into(), json!(q.match_string));
    identity.insert(
        "mode".into(),
        json!(match q.result_view {
            LocalSearchQueryResultView::Detailed => "detailed",
            _ => "paginated",
        }),
    );
    identity.insert(
        "regex".into(),
        json!(match q.regex_mode() {
            LocalSearchQueryRegex::Literal => "fixed",
            LocalSearchQueryRegex::Pcre2 => "perl",
            LocalSearchQueryRegex::Rust => "smart",
        }),
    );
    identity.insert(
        "caseMode".into(),
        json!(match q.case_mode {
            LocalSearchQueryCaseMode::Sensitive => "sensitive",
            LocalSearchQueryCaseMode::Insensitive => "insensitive",
            LocalSearchQueryCaseMode::Smart => "smart",
        }),
    );
    identity.insert(
        "contextLines".into(),
        json!(
            q.context_lines()
                .unwrap_or_else(|| default_context_lines(q.result_view))
        ),
    );
    identity.insert(
        "matchContentLength".into(),
        json!(effective_match_content_length(q)),
    );
    identity.insert(
        "multiline".into(),
        json!(match q.multiline {
            LocalSearchQueryMultiline::Off => "off",
            LocalSearchQueryMultiline::On => "on",
            LocalSearchQueryMultiline::Dotall => "dotall",
        }),
    );
    identity.insert(
        "sort".into(),
        json!(match q.sort {
            LocalSearchQuerySort::Relevance => "relevance",
            LocalSearchQuerySort::Traversal => "traversal",
            LocalSearchQuerySort::MatchCount => "matchCount",
            LocalSearchQuerySort::Path => "path",
            LocalSearchQuerySort::Modified => "modified",
            LocalSearchQuerySort::Accessed => "accessed",
            LocalSearchQuerySort::Created => "created",
        }),
    );
    identity.insert(
        "output".into(),
        json!(match q.result_view {
            LocalSearchQueryResultView::MatchOnly => "matchOnly",
            LocalSearchQueryResultView::Files => "files",
            LocalSearchQueryResultView::FilesWithout => "filesWithout",
            LocalSearchQueryResultView::CountLines => "countLines",
            LocalSearchQueryResultView::CountMatches => "countMatches",
            _ => "content",
        }),
    );
    identity.insert(
        "unique".into(),
        json!(match q.unique {
            LocalSearchQueryUnique::Off => "off",
            LocalSearchQueryUnique::List => "list",
            LocalSearchQueryUnique::Count => "count",
        }),
    );
    macro_rules! opt {
        ($name:literal,$value:expr) => {
            if let Some(v) = $value {
                identity.insert($name.into(), json!(v));
            }
        };
    }
    opt!("wholeWord", q.whole_word);
    opt!("invertMatch", q.invert_match);
    opt!("include", (!q.include.is_empty()).then_some(&q.include));
    opt!("exclude", (!q.exclude.is_empty()).then_some(&q.exclude));
    opt!("noIgnore", q.no_ignore);
    opt!("hidden", q.hidden);
    opt!("maxDepth", q.max_depth);
    opt!("language", q.language.as_ref());
    opt!("sortReverse", q.reverse);
    let mut entries = identity.into_iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    crate::digest::json_sha256(&json!([root.to_string_lossy(), entries]))
}
