//! A match page's continuations: its file, match and scan pages, the value
//! expansions, the read of its top hits, and the terminal end of a query
//! some grammar of the scope does not compile.
use super::clipped::{read_clipped_lines, read_top_hits};
use super::query::MatchQuery;
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
    crate::response::paging::terminal_limit(out, "compileFailed", &[]);
    if let Some(language) = scope.inferred.as_deref().filter(|_| q.language().is_none()) {
        let row = crate::response::result::with_changes(q.to_value(), &json!({"parser": language}));
        out["calls"]["narrowScope"] =
            crate::response::result::restart_query(ToolId::AstSearch, &row);
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
        out["calls"] = json!({"nextPage":with(json!({"page":next_page,"matchPage":1}))});
    }
    if let Some(next_match_page) = next_match_page {
        out["calls"]["nextMatchPage"] = with(json!({"matchPage":next_match_page}));
    }
    if let Some(expand) = expand_scan.filter(|_| !more && !has_more_matches) {
        out["calls"]["expandScan"] = with(expand);
    }
    // A clipped value is display clipping of a listed match, not a coverage
    // gap: expandCaptures re-runs with captures and the whole length when
    // captures are hidden; otherwise (or past the length maximum) the
    // clipped rows' lines are read whole.
    if has_truncated_captures && !q.capture_text().unwrap_or(false) {
        out["calls"]["expandCaptures"] = with(json!({"includeCaptures":true}));
    }
    read_clipped_lines(scope, out, page_groups);
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
        .position(|line| anchors.iter().any(|anchor| spells_word(line, anchor)))
        .map(|index| index as u64 + 1)
}

/// Whether `line` holds `anchor` with no word character joined to either
/// end (an end that is itself punctuation needs no boundary).
fn spells_word(line: &str, anchor: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let (Some(first), Some(last)) = (anchor.chars().next(), anchor.chars().next_back()) else {
        return false;
    };
    line.match_indices(anchor).any(|(start, _)| {
        let before = line[..start].chars().next_back();
        let after = line[start + anchor.len()..].chars().next();
        (!word(first) || !before.is_some_and(word)) && (!word(last) || !after.is_some_and(word))
    })
}

#[cfg(test)]
#[test]
fn an_anchor_is_spelled_as_a_whole_word() {
    assert!(spells_word("fn failure() {}", "failure"));
    assert!(!spells_word("export default x;", "def"));
    assert!(!spells_word("failures.push(x)", "failure"));
    assert!(spells_word("a => b", "=>"));
    assert!(spells_word("x=>y", "=>"));
}
