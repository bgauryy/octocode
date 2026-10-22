//! Continuations and snapshot identity: the next-page calls a result
//! names and the fingerprint that proves a cursor still answers its query.

use super::executor::*;
use super::layout::*;
use super::types::*;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::Confidence;
use crate::response::result::Continuation;
use crate::response::result::ToolError;
use octocode_contract_types::ErrorCode;
use octocode_engine::digest::canonicalize_dropping_nulls;
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
            && (query.snapshot.is_some()
                || page < layout.total_pages
                || (!streamed
                    && parsed
                        .files
                        .iter()
                        .any(|f| f.matches.len() as u32 > layout.matches_per)))))
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
        selection,
    );
    (next, leftover_matches)
}

/// A continuation whose snapshot no longer describes the source; `next.restart`
/// reruns the query from its first page, without any cursor.
pub(super) fn stale_snapshot(query: &LocalSearchQuery) -> ToolError {
    ToolError::stale_snapshot(crate::response::result::restart_query(
        ToolId::LocalSearch,
        &normalized_query(query, streamed_layout(query)),
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

/// The query as continuations carry it. The window fields (`contextLines`,
/// `matchContentLength`) stay as the caller sent them: every call derives
/// their defaults from the view the same way, and so does the snapshot
/// identity. A streamed layout carries no `pageSize`: its pages are cut by
/// the budget.
pub(super) fn normalized_query(q: &LocalSearchQuery, streamed: bool) -> Value {
    let mut value = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    // `LocalSearchQuery` serializes to a JSON object.
    #[expect(
        clippy::expect_used,
        reason = "the request is a JSON object by construction"
    )]
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
    selection: Option<&str>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();
    let mut base = normalized_query(q, streamed);
    if let Some(token) = selection {
        base["selection"] = json!(token);
    }
    if let Some(snapshot) = snapshot {
        base["snapshot"] = json!(snapshot);
    }
    // A `nextPage` continuation is never emitted past the contract page
    // maximum; callers must narrow the query to reach later results.
    let cursor = |field: &str, current: u32| {
        crate::response::paging::admitted(ToolId::LocalSearch, None, field, current as usize + 1)
    };
    let next_match_page = leftover_matches
        .then(|| cursor("matchPage", match_page))
        .flatten();
    // Rows left in the shown files (`nextMatchPage`) and later files
    // (`nextPage`) are both offered on the page that opens a file group
    // (matchPage 1), so the caller goes depth-first or breadth-first. A
    // later match page of the same group does not offer `nextPage` again:
    // that call was already offered, and every executable query appears
    // once in the chain. A chain's first call (no snapshot yet) offers it
    // at any match page, since no earlier page did.
    let opens_file_group = match_page <= 1 || local_search_query::snapshot(q).is_none();
    if page < total_pages
        && opens_file_group
        && let Some(n) = crate::response::paging::admitted_query(
            ToolId::LocalSearch,
            None,
            &base,
            "page",
            page,
            &[("matchPage", 1)],
        )
    {
        map.insert(
            "nextPage".into(),
            Continuation::new(ToolId::LocalSearch, n)
                .confidence(Confidence::Exact)
                .build(),
        );
    }
    if let Some(next_match_page) = next_match_page {
        let mut n = base;
        n["matchPage"] = json!(next_match_page);
        map.insert(
            "nextMatchPage".into(),
            Continuation::new(ToolId::LocalSearch, n)
                .confidence(Confidence::Exact)
                .build(),
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
const NON_SEMANTIC_FIELDS: [&str; 10] = [
    "page",
    "matchPage",
    "pageSize",
    "matchPageSize",
    "snapshot",
    "selection",
    "clasifyContext",
    "extras",
    "mainGoal",
    "reasoning",
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
    let mut semantics = normalized_query(&local_search_query::canonical(q), true);
    if let Some(fields) = semantics.as_object_mut() {
        for field in NON_SEMANTIC_FIELDS {
            fields.remove(field);
        }
        fields.remove("path");
        fields.insert("regex".into(), json!(local_search_query::regex_mode(q)));
        let context = local_search_query::context(q);
        fields.insert(
            "contextLines".into(),
            json!(if context.before == context.after {
                context.before
            } else {
                0
            }),
        );
        fields.insert(
            "matchContentLength".into(),
            json!(effective_match_content_length(q)),
        );
        fields.insert(
            "defaultExcludes".into(),
            json!(q.default_excludes.defaults()),
        );
        // The page cut: a continuation served under another response window
        // would cut other pages, so it restarts instead of skipping rows.
        fields.insert("pageChars".into(), json!(page_budget));
        // Span rows are priced with `column` only while the response keeps
        // it, which `extras` decides: that, too, cuts other pages.
        fields.insert(
            "spanColumns".into(),
            json!(super::layout::shows_span_columns(q)),
        );
    }
    crate::digest::json_sha256(&canonicalize_dropping_nulls(json!([
        root.to_string_lossy(),
        semantics
    ])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_engine::types::{
        SearchedSource, TextSearchFile, TextSearchMatch, TextSearchStats,
    };

    fn scan() -> (Vec<TextSearchFile>, TextSearchStats) {
        let hit = |line: u32, value: &str| TextSearchMatch {
            deferred: None,
            line,
            last_line: None,
            column: 3,
            byte_offset: 3,
            value: value.into(),
            value_digest: [line as u8; 32],
            count: None,
            rank: Some(1),
            original_chars: None,
            original_bytes: None,
        };
        let files = vec![
            TextSearchFile {
                path: "src/a.rs".into(),
                match_count: 2,
                matches: vec![hit(1, "fn needle()"), hit(9, "needle();")],
                source: Some(SearchedSource {
                    size: 120,
                    modified: Some(std::time::SystemTime::UNIX_EPOCH),
                    digest: [1; 32],
                }),
            },
            TextSearchFile {
                path: "src/b.rs".into(),
                match_count: 1,
                matches: vec![hit(4, "needle")],
                source: None,
            },
        ];
        let stats = TextSearchStats {
            match_count: Some(3),
            matched_lines: Some(3),
            files_matched: Some(2),
            files_searched: Some(10),
            ..TextSearchStats::default()
        };
        (files, stats)
    }

    /// The snapshot binds every collected field of every row, each file's
    /// source digest and the totals, and nothing else: a source's size and
    /// time alone (its bytes unchanged) keep it. It is a v8 snapshot a
    /// later call accepts as well formed.
    #[test]
    fn the_snapshot_binds_every_row_field_and_no_stamp() {
        let (files, stats) = scan();
        let base = fingerprint("key", &files, &stats);
        assert!(base.starts_with("lexical-live-v8:"), "{base}");
        assert_eq!(fingerprint("key", &files, &stats), base);
        let snapshot = serde_json::json!({"path": ".", "matchString": "x", "mainGoal": "t", "reasoning": "t", "snapshot": base});
        let query: LocalSearchQuery = serde_json::from_value(snapshot).expect("query");
        assert!(malformed_snapshot(&query).is_none());

        type Edit = fn(&mut Vec<TextSearchFile>, &mut TextSearchStats);
        let bound: [(&str, Edit); 18] = [
            ("path", |f, _| f[0].path.push('x')),
            ("matchCount", |f, _| f[0].match_count += 1),
            ("line", |f, _| f[0].matches[0].line += 1),
            ("column", |f, _| f[0].matches[0].column += 1),
            ("byteOffset", |f, _| f[0].matches[0].byte_offset += 1),
            ("value", |f, _| f[0].matches[0].value.push('x')),
            ("valueDigest", |f, _| f[0].matches[0].value_digest[0] ^= 1),
            ("rank", |f, _| f[0].matches[0].rank = Some(3)),
            ("originalChars", |f, _| {
                f[0].matches[0].original_chars = Some(9)
            }),
            ("originalBytes", |f, _| {
                f[0].matches[0].original_bytes = Some(9)
            }),
            ("count", |f, _| f[0].matches[0].count = Some(1)),
            ("sourceDigest", |f, _| {
                if let Some(source) = &mut f[0].source {
                    source.digest[0] ^= 1;
                }
            }),
            ("source", |f, _| {
                f[1].source = Some(SearchedSource {
                    size: 0,
                    modified: None,
                    digest: [0; 32],
                })
            }),
            ("rows", |f, _| {
                let row = f[0].matches.remove(1);
                f[1].matches.push(row);
            }),
            ("matchTotal", |_, s| s.match_count = Some(4)),
            ("filesScanned", |_, s| s.files_searched = Some(11)),
            ("capReason", |_, s| {
                s.cap_reasons = vec![octocode_engine::types::CapReason::MaxCollectedFiles]
            }),
            ("firstError", |_, s| s.first_error = Some("denied".into())),
        ];
        for (field, edit) in bound {
            let (mut files, mut stats) = scan();
            edit(&mut files, &mut stats);
            assert_ne!(fingerprint("key", &files, &stats), base, "{field}");
        }
        assert_ne!(fingerprint("key2", &files, &stats), base, "query key");
        // Text boundaries cannot shift between fields.
        let (mut shifted, _) = scan();
        shifted[0].path = "src/a.r".into();
        shifted[0].matches[0].value = "sfn needle()".into();
        assert_ne!(fingerprint("key", &shifted, &stats), base);

        let (mut stamped, _) = scan();
        if let Some(source) = &mut stamped[0].source {
            source.size = 999;
            source.modified = None;
        }
        assert_eq!(fingerprint("key", &stamped, &stats), base, "stamps only");
        // An error count of zero is no error, as before.
        let mut zero = stats.clone();
        zero.error_count = Some(0);
        assert_eq!(fingerprint("key", &files, &zero), base);
    }
}
