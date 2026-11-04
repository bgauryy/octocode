//! A match page's continuations: its file, match and scan pages, the value
//! expansions and the read of its top hits.
use super::query::MatchQuery;
use super::query::Move;
use super::reads::read_top_hits;
use super::rows::continuation_with;
use super::types::{Group, ScanCut, Scope};
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::response::result::insert_next;
use octocode_contract_types::{
    AstSearchChannel, AstSearchQuerySyntaxTree, AstSearchQuerySyntaxTreeOperation, ErrorCode,
};
use serde_json::{Value, json};

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
) -> Result<(), ToolError> {
    let truncated = cut.truncated;
    let page = u64::from(q.page().max(1));
    let match_page = u64::from(q.match_page().max(1));
    let has_more_matches = page_groups.iter().any(|group| group.more_matches);
    let has_truncated_captures = page_groups.iter().any(|group| group.truncated_captures);
    // The scan continues after the files this window evaluated
    // (`scanOffset`), so the lead is offered once the window's last file page
    // is reached.
    let expand_scan = super::super::scan_window::expand_scan(truncated, cut.resume)
        .map(|window| u64::from(window.scan_offset));
    // Finish each file page's matches before advancing the file/scan axis.
    // Otherwise every match page reissues the same next file or scan window.
    // The row's own walk therefore leads to the split files' next match page,
    // and to the next file page only once every file on this page has shown
    // its last match. A split file's pagination states only that it has more.
    // A continuation that cannot be built is a defect of this response, not
    // an absent lead: `hasMore` never stands without its call.
    let with = |to: Move| {
        continuation_with(q, to, snapshot).ok_or_else(|| {
            ToolError::new(
                ErrorCode::OutputContractViolation,
                format!("astSearch could not build the continuation for {to:?}"),
            )
        })
    };
    let row_call = if has_more_matches {
        // One call reads the next match page of every split file on this
        // page, so the row's walk holds it and no file's pager repeats it.
        Some(with(Move::MatchPage(match_page + 1))?)
    } else if more {
        // A new file page restarts per-file match pagination.
        Some(with(Move::FilePage(page + 1))?)
    } else {
        None
    };
    if let Some(call) = row_call {
        out["calls"] = json!({});
        insert_next(&mut out["calls"], AstSearchChannel::NextPage, call);
        // The row's `pagination` says its walk goes on, though every file
        // is listed when a match page is what remains.
        if let Some(pagination) = out.get_mut("pagination").and_then(Value::as_object_mut) {
            pagination.insert("hasMore".into(), json!(true));
        }
    }
    if let Some(offset) = expand_scan.filter(|_| !more && !has_more_matches) {
        insert_next(
            &mut out["calls"],
            AstSearchChannel::ExpandScan,
            with(Move::ScanWindow(offset))?,
        );
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
    Ok(())
}
/// A query that does not compile against one file leads to that file's
/// syntax tree, where the node shapes a pattern or rule names are read.
pub(super) fn tree_repair(mut error: ToolError, q: MatchQuery<'_>, scope: &Scope) -> ToolError {
    if error.code == ErrorCode::InvalidPattern && error.calls.is_none() {
        error.calls = tree_lead(q, scope, &scope.canonical, None)
            .map(|lead| Box::new(json!({ "viewSyntax": lead })));
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
) -> Option<Value> {
    // One bounded page: the nodes at `line` (the statement that spells the
    // query's anchor), else the file's first nodes, with the rest a page on.
    let tree = AstSearchQuerySyntaxTree {
        include_provenance: None,
        line: line.and_then(std::num::NonZeroU64::new),
        main_goal: None,
        named_only: true,
        operation: AstSearchQuerySyntaxTreeOperation::SyntaxTree,
        page: std::num::NonZeroU64::MIN,
        parser: match q.language().filter(|_| scope.is_file) {
            Some(language) => Some(language.parse().ok()?),
            None => None,
        },
        path: file.to_string_lossy().as_ref().try_into().ok()?,
        reasoning: None,
        snapshot: None,
    };
    Some(crate::response::result::Continuation::related_query(tree).build())
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
