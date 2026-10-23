//! A match page's continuations: its file, match and scan pages, the value
//! expansions, the read of its top hits, and the terminal end of a query
//! some grammar of the scope does not compile.
use super::clipped::{read_clipped_lines, read_top_hits};
use super::query::MatchQuery;
use super::rows::continuation_with;
use super::types::{Group, ScanCut, Scope};
use crate::contracts::query_limits::ast_search::r#match as limits;
use crate::id::ToolId;
use crate::response::result::ToolError;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

/// Files whose grammar the query does not compile in are on no page of any
/// call with this query: a terminal limit (their diagnostics name them and
/// the repair). When one grammar of the scope compiles it, `narrowScope`
/// is the same query over that grammar's files alone, a result with no such
/// gap.
pub(super) fn compile_failed_end(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    snapshot: &str,
) {
    crate::response::paging::terminal_limit(out, "compileFailed", &[]);
    if let Some(language) = scope.inferred.as_deref().filter(|_| q.language().is_none()) {
        out["next"]["narrowScope"] = continuation_with(
            q,
            json!({"language": language, "page": 1, "matchPage": 1, "scanOffset": null, "snapshot": null}),
            snapshot,
        );
    }
}

/// `next` pages, the scan and value expansions, and the read of the top
/// file's hits.
pub(super) fn attach_match_continuations(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    page_groups: &[Group],
    cut: ScanCut<'_>,
    more: bool,
    snapshot: &str,
) {
    let truncated = cut.truncated;
    let page = q.page().max(1) as usize;
    let match_page = q.match_page().max(1) as usize;
    let has_more_matches = page_groups.iter().any(|group| group.more_matches);
    let has_truncated_captures = page_groups.iter().any(|group| group.truncated_captures);
    let cut_chars = page_groups.iter().filter_map(|group| group.cut_chars).max();
    let max_content_length = limits::MATCH_CONTENT_LENGTH_MAXIMUM;
    let content_length = q
        .match_content_length()
        .map_or(limits::MATCH_CONTENT_LENGTH_DEFAULT, |length| {
            length as usize
        })
        .clamp(1, limits::MATCH_CONTENT_LENGTH_MAXIMUM);
    // The scan continues after the files this window evaluated
    // (`scanOffset`), so the lead is offered once the window's last file page
    // is reached.
    let expand_scan = super::super::scan_window::expand_scan(
        truncated,
        q.max_files(),
        cut.resume,
        super::super::scan_window::ScanLimits::MATCH,
    )
    .map(|window| {
        let mut expand =
            json!({"scanOffset":window.scan_offset,"page":1,"matchPage":1,"snapshot":null});
        if let Some(bound) = window.max_files {
            expand["maxFiles"] = json!(bound);
        }
        expand
    });
    if truncated && !more && expand_scan.is_none() {
        crate::response::paging::terminal_limit(
            out,
            super::super::scan_window::cut_reason(q.max_files()),
            &[],
        );
        if let Some(unwalked) = cut.unwalked {
            let query = q.to_value();
            unwalked.attach(out, |reach| super::super::unwalked::narrowed(&query, reach));
        }
    }
    let cursor = |out: &mut Value, field: &str, next: usize| {
        crate::response::paging::next_cursor(out, ToolId::AstSearch, Some("match"), field, next)
    };
    // Finish each file page's matches before advancing the file/scan axis.
    // Otherwise every match page reissues the same next file or scan window.
    let next_page = (more && !has_more_matches)
        .then(|| cursor(out, "page", page + 1))
        .flatten();
    let next_match_page = has_more_matches
        .then(|| cursor(out, "matchPage", match_page + 1))
        .flatten();
    let with = |changes: Value| continuation_with(q, changes, snapshot);
    if let Some(next_page) = next_page {
        // A new file page restarts per-file match pagination.
        out["next"] = json!({"nextPage":with(json!({"page":next_page,"matchPage":1}))})
    }
    if let Some(next_match_page) = next_match_page {
        out["next"]["nextMatchPage"] = with(json!({"matchPage":next_match_page}));
    }
    if let Some(expand) = expand_scan.filter(|_| !more && !has_more_matches) {
        out["next"]["expandScan"] = with(expand);
    }
    // A clipped value is display clipping of a listed match, not a coverage
    // gap: expandCaptures re-runs with captures and the whole length when
    // captures are hidden; otherwise (or past the length maximum) the
    // clipped rows' lines are read whole.
    let whole_length = cut_chars.map(|chars| chars.min(max_content_length));
    let expand_captures = has_truncated_captures && !q.capture_text().unwrap_or(false);
    if expand_captures {
        let mut expand = json!({"captureText":true});
        if let Some(length) = whole_length.filter(|length| *length > content_length) {
            expand["matchContentLength"] = json!(length);
        }
        out["next"]["expandCaptures"] = with(expand);
    }
    if !expand_captures || cut_chars.is_some_and(|chars| chars > max_content_length) {
        read_clipped_lines(scope, out, page_groups);
    }
    if let Some(read) = page_groups
        .first()
        .and_then(|top| read_top_hits(scope, &top.row))
    {
        out["next"]["read"] = read;
    }
}
/// A query that does not compile against one file leads to that file's
/// syntax tree, where the node shapes a pattern or rule names are read.
pub(super) fn tree_repair(mut error: ToolError, q: MatchQuery<'_>, scope: &Scope) -> ToolError {
    if error.code == ErrorCode::InvalidPattern && error.next.is_none() {
        error.next = Some(Box::new(
            json!({ "viewSyntax": tree_lead(q, scope, &scope.canonical, None) }),
        ));
    }
    error
}

/// The syntaxTree read of `file` (its grammar pinned when one was named
/// for a single file).
pub(super) fn tree_lead(
    q: MatchQuery<'_>,
    scope: &Scope,
    file: &std::path::Path,
    line: Option<u64>,
) -> Value {
    // One bounded page: the nodes at `line` (the statement that spells the
    // query's anchor), else the file's first nodes, with the rest a page on.
    let mut tree =
        json!({"operation":"syntaxTree","path":file.to_string_lossy(),"pageSize":TREE_LEAD_NODES});
    if let Some(line) = line {
        tree["line"] = json!(line);
    }
    if scope.is_file
        && let Some(language) = q.language()
    {
        tree["language"] = json!(language);
    }
    crate::response::result::Continuation::new(ToolId::AstSearch, tree).build()
}

/// Nodes on the first page of a `viewSyntax` lead.
const TREE_LEAD_NODES: u32 = 40;

/// The first line of `file` that spells a literal every match must contain:
/// where a parsed file's tree is compared with the query's shape.
pub(super) fn anchor_line(q: MatchQuery<'_>, file: &std::path::Path) -> Option<u64> {
    let anchors =
        octocode_engine::structural::query_anchors(q.pattern().as_deref(), q.rule().as_ref())?;
    let source = super::super::source::read_parse_source(file)
        .ok()
        .flatten()?;
    super::super::source::parse_text(file, &source)
        .ok()?
        .lines()
        .position(|line| anchors.iter().any(|anchor| line.contains(anchor.as_str())))
        .map(|index| index as u64 + 1)
}
