//! The query as the walk and every page read it: the wire contract's
//! fields in engine units, the defaults an omitted field takes, and the
//! normalized query continuations carry. The engine counts in `u32`; the
//! wire contract owns the field set and its JSON integer types.

use crate::id::ToolId;
use crate::id::local_search_policy as policy;
use crate::num::{u32_of, u32_of_signed};
use crate::policy::prune::DefaultsFlag;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::{
    LocalSearchQuery, LocalSearchQueryCaseMode, LocalSearchQueryRegex, LocalSearchQueryResultView,
    MatchString,
};
use octocode_engine::types::SearchContext;
use serde_json::{Value, json};

/// Context lines each side of a hit when `clasifyContext` judges hits
/// and no context field is set: a bare hit line (`fn prunes(..) {`)
/// reads as related at best, while the hit with its body shows what it
/// does, so the judge can select it.
pub(crate) const CLASIFY_CONTEXT_LINES: u32 = crate::id::CLASIFY_CONTEXT_LINES;

/// Resolve the shorthand once; explicit zero overrides either side.
pub(crate) fn context(query: &LocalSearchQuery) -> SearchContext {
    let unset = query.context_lines.is_none()
        && query.context_before.is_none()
        && query.context_after.is_none();
    let fallback = if unset && query.clasify_context.is_some() {
        CLASIFY_CONTEXT_LINES
    } else {
        0
    };
    let symmetric = query.context_lines.map(u32_of_signed).unwrap_or(fallback);
    SearchContext {
        before: query.context_before.map(u32_of_signed).unwrap_or(symmetric),
        after: query.context_after.map(u32_of_signed).unwrap_or(symmetric),
    }
}
pub(crate) fn match_content_length(query: &LocalSearchQuery) -> Option<u32> {
    query.match_content_length.map(|n| u32_of(n.get()))
}
/// The caller's per-file row cap; `None` lets the result size decide.
pub(crate) fn match_page_size(query: &LocalSearchQuery) -> Option<u32> {
    query.match_page_size.map(|n| u32_of(n.get()))
}
/// The walk depth below `path` in engine terms (0 = files directly in
/// `path`): the wire `maxDepth` counts levels, 1 = its children.
pub(crate) fn max_depth(query: &LocalSearchQuery) -> Option<u32> {
    query
        .max_depth
        .map(|depth| u32_of(depth.get()).saturating_sub(1))
}
pub(crate) fn page(query: &LocalSearchQuery) -> u32 {
    u32_of(query.page.get())
}
pub(crate) fn match_page(query: &LocalSearchQuery) -> u32 {
    u32_of(query.match_page.get())
}
pub(crate) fn page_size(query: &LocalSearchQuery) -> Option<u32> {
    query.page_size.map(|n| u32_of(n.get()))
}
pub(crate) fn snapshot(query: &LocalSearchQuery) -> Option<&str> {
    query.snapshot.as_deref().map(String::as_str)
}
/// The search mode: the caller's `regex`, else literal text in every tool
/// (`a|b` finds that text; `regex:"rust"` runs it as a pattern).
pub(crate) fn regex_mode(query: &LocalSearchQuery) -> LocalSearchQueryRegex {
    query.regex.unwrap_or(LocalSearchQueryRegex::Literal)
}

/// The text the search runs: the one `matchString`. A list is only ever
/// seen here after [`LocalSearchQuery::normalized`] folded it into its alternation.
pub(crate) fn pattern(query: &LocalSearchQuery) -> &str {
    match &query.match_string {
        MatchString::String(one) => one.as_str(),
        MatchString::Array(_) => unreachable!("a list matchString is normalized first"),
    }
}

/// The query with a list `matchString` as the one regex query that finds
/// the same lines (`tools::match_list`): each entry literal text unless
/// `regex` names a dialect, each judged by `caseMode` on its own (smart:
/// an uppercase letter makes that entry sensitive). Everything after
/// this, leads and cursors included, sees an ordinary regex search that
/// runs as written. A single `matchString`, or a list of one, is that
/// text as written.
pub(crate) fn normalized(
    query: &LocalSearchQuery,
) -> Result<std::borrow::Cow<'_, LocalSearchQuery>, crate::response::result::ToolError> {
    let MatchString::Array(list) = &query.match_string else {
        return Ok(std::borrow::Cow::Borrowed(query));
    };
    // One entry is that text itself: it keeps its literal or regex
    // reading (and the leads that read it), as a string would.
    if let [only] = list.as_slice() {
        let text: &str = only;
        let mut query = query.clone();
        query.match_string = MatchString::String(text.parse().map_err(|_| {
            crate::response::result::ToolError::new(
                ErrorCode::InvalidInput,
                "matchString: an entry needs text.",
            )
        })?);
        return Ok(std::borrow::Cow::Owned(query));
    }
    let sensitive = |entry: &str| match query.case_mode {
        LocalSearchQueryCaseMode::Smart => entry.chars().any(char::is_uppercase),
        LocalSearchQueryCaseMode::Sensitive => true,
        LocalSearchQueryCaseMode::Insensitive => false,
    };
    let dialect = match regex_mode(query) {
        LocalSearchQueryRegex::Pcre2 => LocalSearchQueryRegex::Pcre2,
        LocalSearchQueryRegex::Literal | LocalSearchQueryRegex::Rust => {
            LocalSearchQueryRegex::Rust
        }
    };
    let alternation = crate::tools::match_list::alternation(
        list.iter().map(|entry| {
            let text: &str = entry;
            (text, sensitive(text))
        }),
        regex_mode(query) != LocalSearchQueryRegex::Literal,
    );
    let mut query = query.clone();
    query.match_string = MatchString::String(alternation.parse().map_err(|_| {
        crate::response::result::ToolError::new(
            ErrorCode::InvalidInput,
            "matchString: a list needs at least one entry.",
        )
    })?);
    query.regex = Some(dialect);
    query.case_mode = LocalSearchQueryCaseMode::Sensitive;
    Ok(std::borrow::Cow::Owned(query))
}

/// The query with every value that means what the field's absence means
/// dropped: an off flag (`includeHidden:false`), `regex:"literal"`,
/// `defaultExcludes:true`, `contextLines:0` and the default
/// `matchContentLength`. The snapshot key and the page cut are derived
/// from it, so a cursor never goes stale because a caller spelled a
/// default; continuations still carry the fields as the caller sent them.
pub(crate) fn canonical(query: &LocalSearchQuery) -> LocalSearchQuery {
    use crate::policy::prune::DefaultsFlag;
    let mut query = query.clone();
    for flag in [
        &mut query.include_hidden,
        &mut query.include_ignored,
        &mut query.invert_match,
        &mut query.reverse,
        &mut query.whole_word,
    ] {
        if *flag == Some(false) {
            *flag = None;
        }
    }
    if query.regex == Some(LocalSearchQueryRegex::Literal) {
        query.regex = None;
    }
    if query.default_excludes.defaults() {
        query.default_excludes = None;
    }
    let context = context(&query);
    let symmetric = context.before == context.after;
    query.context_lines =
        (symmetric && context.before > 0).then_some(i64::from(context.before));
    query.context_before =
        (!symmetric && context.before > 0).then_some(i64::from(context.before));
    query.context_after = (!symmetric && context.after > 0).then_some(i64::from(context.after));
    let length = query.match_content_length.take();
    if length.map(|n| u32_of(n.get()))
        != Some(effective_match_content_length(&query))
    {
        query.match_content_length = length;
    }
    query
}

/// Soft budget (in value chars) for a single localSearch response body.
/// Distributed across the matches shown on a page so a pathological giant line
/// or a raised `matchContentLength`/`matchPageSize` cannot emit a multi-MB
/// body. It bounds displayed value size only: every match row and its line
/// anchor are preserved, and a clipped row's `expandValues` lead reads its
/// value whole.
pub(super) const RESPONSE_VALUE_CHAR_BUDGET: usize = 1_000_000;

/// Floor on the per-match display cap the budget may impose, so a page with many
/// matches still shows a useful slice of each rather than a few characters.
pub(super) const MIN_MATCH_VALUE_CHARS: usize = 40;

/// Lean agent-facing defaults (caller values always win), authored once in
/// the contract (`localSearch.semanticPolicy`): one clipped-around-the-hit
/// line per row is enough to pick the file+line.
pub(super) const DEFAULT_MATCH_CONTENT_LENGTH: u32 = policy::MATCH_CONTENT_LENGTH as u32;

/// Cap on the context-scaled default; an explicit matchContentLength may exceed it.
pub(super) const MAX_DEFAULT_MATCH_CONTENT_LENGTH: u32 = policy::MAX_MATCH_CONTENT_LENGTH as u32;

/// Rows per file when a result is too large to show whole.
pub(super) const DEFAULT_MAX_MATCHES_PER_FILE: u32 = policy::MATCHES_PER_FILE as u32;

/// Files per page for snippet views (the first page of a default-layout
/// result larger than the budget); path-only list views use the list size.
pub(super) const DEFAULT_SNIPPET_PAGE_SIZE: u32 = policy::SNIPPET_PAGE_SIZE as u32;

pub(super) const DEFAULT_LIST_PAGE_SIZE: u32 = policy::LIST_PAGE_SIZE as u32;

/// Matched files the engine collects for pages; more are counted only.
pub(super) const MAX_COLLECTED_FILES: u32 = 10_000;

/// Per-hit character budget. An omitted matchContentLength scales with the
/// effective context window so requested context is not silently clipped;
/// continuations and snapshot identity must use this same value.
pub(super) fn effective_match_content_length(q: &LocalSearchQuery) -> u32 {
    match_content_length(q).unwrap_or_else(|| {
        let view = q.result_view;
        let context = if uses_context(view) {
            context(q)
                .before
                .saturating_add(context(q).after)
        } else {
            0
        };
        DEFAULT_MATCH_CONTENT_LENGTH
            .saturating_mul(context.saturating_add(1))
            .min(MAX_DEFAULT_MATCH_CONTENT_LENGTH)
    })
}

/// Default files per page: snippet views stay lean; path-only views are cheap.
pub(super) fn default_page_size(view: LocalSearchQueryResultView) -> u32 {
    match view {
        LocalSearchQueryResultView::Files
        | LocalSearchQueryResultView::FilesWithout
        | LocalSearchQueryResultView::CountLines
        | LocalSearchQueryResultView::CountMatches => DEFAULT_LIST_PAGE_SIZE,
        _ => DEFAULT_SNIPPET_PAGE_SIZE,
    }
}

/// Views whose match rows carry a context window (so `contextLines` matters).
pub(super) fn uses_context(view: LocalSearchQueryResultView) -> bool {
    view == LocalSearchQueryResultView::Paginated
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

/// Default layouts use the response budget when neither page axis is sized.
pub(super) fn streamed_layout(q: &LocalSearchQuery) -> bool {
    page_size(q).is_none()
        && match_page_size(q).is_none()
        && match_page(q) <= 1
}

/// The response path of a span row's `column`.
pub(super) const SPAN_COLUMN_PATH: &str = "results[].data.files[].matches[].column";

/// Whether the response keeps `column` on this query's rows: only matchOnly
/// span rows carry one, and only while the output stage keeps the field
/// ([`crate::response::extras::keeps`]: core may class it an extra its
/// `extras` kind must name). A page is priced for what it emits.
pub(super) fn shows_span_columns(query: &LocalSearchQuery) -> bool {
    query.result_view == LocalSearchQueryResultView::MatchOnly
        && serde_json::to_value(query).is_ok_and(|row| {
            crate::response::extras::keeps(ToolId::LocalSearch, &row, SPAN_COLUMN_PATH)
        })
}

#[cfg(test)]
mod context_tests {
    use super::*;

    /// `clasifyContext` judges each hit with context unless the caller set a
    /// context field; an explicit 0 keeps bare hit lines.
    #[test]
    fn clasify_hits_get_context_unless_a_context_field_is_set() {
        let query = |extra: serde_json::Value| -> LocalSearchQuery {
            let mut row = serde_json::json!({"path": ".", "matchString": "x"});
            row.as_object_mut()
                .expect("object")
                .extend(extra.as_object().expect("object").clone());
            serde_json::from_value(row).expect("query")
        };
        let clasify = serde_json::json!({"classifiers": ["Q?"]});
        let lines = CLASIFY_CONTEXT_LINES;
        assert_eq!(
            context(&query(serde_json::json!({"clasifyContext": clasify}))),
            SearchContext::symmetric(lines)
        );
        assert_eq!(
            context(&query(
                serde_json::json!({"clasifyContext": clasify, "contextLines": 0})
            )),
            SearchContext::symmetric(0)
        );
        assert_eq!(
            context(&query(serde_json::json!({}))),
            SearchContext::symmetric(0)
        );
    }
}
