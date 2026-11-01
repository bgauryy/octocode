//! A match page's continuations: its file, match and scan pages, the value
//! expansions, the read of its top hits, and the terminal end of a query
//! some grammar of the scope does not compile.
use super::query::MatchQuery;
use super::reads::read_top_hits;
use super::rows::continuation_with;
use super::types::{Group, ScanCut, Scope};
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
    _snapshot: &str,
) {
    crate::response::paging::terminal_limit(
        out,
        crate::response::paging::PartialReason::CompileFailed,
        &[],
    );
    if let Some(language) = scope.inferred.as_deref().filter(|_| q.language().is_none()) {
        let row = crate::response::result::with_changes(q.to_value(), &json!({"parser": language}));
        out["calls"]["narrowScope"] =
            crate::response::result::restart_query(ToolId::AstSearch, &row);
    }
}

/// `next` pages and the scan expansion; the related capture expansion and
/// read of the top file's hits.
pub(super) fn attach_match_continuations(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    page_groups: &[Group],
    cut: ScanCut,
    more: bool,
    snapshot: &str,
) {
    let truncated = cut.truncated;
    let page = q.page().max(1) as usize;
    let match_page = q.match_page().max(1) as usize;
    let has_more_matches = page_groups.iter().any(|group| group.more_matches);
    let has_truncated_captures = page_groups.iter().any(|group| group.truncated_captures);
    // The scan continues after the files this window evaluated
    // (`scanOffset`), so the lead is offered once the window's last file page
    // is reached.
    let expand_scan = super::super::scan_window::expand_scan(truncated, cut.resume).map(
        |window| json!({"scanOffset":window.scan_offset,"page":1,"matchPage":1,"snapshot":null}),
    );
    // Finish each file page's matches before advancing the file/scan axis.
    // Otherwise every match page reissues the same next file or scan window.
    let next_page = (more && !has_more_matches).then_some(page + 1);
    let next_match_page = has_more_matches.then_some(match_page + 1);
    let with = |changes: Value| continuation_with(q, changes, snapshot);
    if let Some(next_page) = next_page {
        // A new file page restarts per-file match pagination.
        out["calls"] = json!({"nextPage":with(json!({"page":next_page,"matchPage":1}))});
    }
    if let Some(next_match_page) = next_match_page {
        out["calls"]["nextMatchPage"] = with(json!({"matchPage":next_match_page}));
    }
    if let Some(expand) = expand_scan.filter(|_| !more && !has_more_matches) {
        out["calls"]["expandScan"] = with(expand);
        super::super::scan_window::mark_resumed(out);
    }
    // The lean structural representation may withhold a body or captures.
    // Expanding that chosen representation returns their complete text:
    // optional work, so a related lead, never a required continuation.
    if has_truncated_captures && !q.capture_text().unwrap_or(false) {
        out["calls"]["expandCaptures"] = crate::response::result::restart_query(
            ToolId::AstSearch,
            &crate::response::result::with_changes(q.to_value(), &json!({"includeCaptures":true})),
        );
    }
    if let Some(read) = page_groups
        .first()
        .and_then(|top| read_top_hits(scope, &top.row))
    {
        out["calls"]["read"] = read;
    }
}
/// A query that does not compile against one file leads to that file's
/// syntax tree, where the node shapes a pattern or rule names are read.
pub(super) fn tree_repair(mut error: ToolError, q: MatchQuery<'_>, scope: &Scope) -> ToolError {
    if error.code == ErrorCode::InvalidPattern && error.calls.is_none() {
        error.calls = Some(Box::new(
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
    let mut tree = json!({"operation":"syntaxTree","path":file.to_string_lossy()});
    if let Some(line) = line {
        tree["line"] = json!(line);
    }
    if scope.is_file
        && let Some(language) = q.language()
    {
        tree["parser"] = json!(language);
    }
    crate::response::result::Continuation::related(ToolId::AstSearch, tree).build()
}

/// The first line of `file` that spells a literal every match must contain,
/// as a whole word (`def` is not in `default`): where a parsed file's tree
/// is compared with the query's shape.
pub(super) fn anchor_line(q: MatchQuery<'_>, file: &std::path::Path) -> Option<u64> {
    let anchors = octocode_engine::structural::query_anchors(None, q.rule().as_ref())?;
    let source = super::super::source::read_parse_source(file)
        .ok()
        .flatten()?;
    super::super::source::parse_text(file, &source)
        .ok()?
        .lines()
        .position(|line| {
            anchors.iter().any(|anchor| {
                octocode_engine::text::whole_word_starts(line, anchor)
                    .next()
                    .is_some()
            })
        })
        .map(|index| index as u64 + 1)
}

/// The anchor check is the engine's identifier rule: an end that is itself
/// punctuation needs no boundary, and `$def` is not `def`.
#[cfg(test)]
#[test]
fn an_anchor_is_spelled_as_a_whole_word() {
    let spells = |line: &str, anchor: &str| {
        octocode_engine::text::whole_word_starts(line, anchor)
            .next()
            .is_some()
    };
    assert!(spells("fn failure() {}", "failure"));
    assert!(!spells("export default x;", "def"));
    assert!(!spells("failures.push(x)", "failure"));
    assert!(spells("a => b", "=>"));
    assert!(spells("x=>y", "=>"));
    assert!(!spells("$def = 1", "def"));
    assert!(spells("(failure)", "failure"));
}
