//! A match page's continuations: its next page and scan window.
use super::query::MatchQuery;
use super::query::Move;
use super::rows::continuation_with;
use super::types::{ScanCut, Scope};
use octocode_contract_types::{
    AstSearchChannel, AstSearchQuerySyntaxTree, AstSearchQuerySyntaxTreeOperation, ErrorCode,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::response::result::insert_next;
use serde_json::{Value, json};

/// The diagnostic code of a file the scan skipped for passing one of its own
/// source limits, the code of the engine's `sourceLimit` skip.
pub(super) const SOURCE_LIMIT_CODE: &str = "structural.source.failed";

/// The query again over each half of the lines it searched in `file`
/// (`display` as the response spells it): its `scope`, or the whole file.
/// Where that range passed a limit of the file's own, each half is a call
/// that can answer, and a half that still passes leads to its own halves, so
/// the chain ends in about log2 steps and reaches every line once. `None`
/// when the range is a single line.
pub(super) fn narrow_leads(
    q: MatchQuery<'_>,
    file: &std::path::Path,
    display: &str,
) -> Option<[Value; 2]> {
    let query = q.to_value();
    let line = |at: &str| query["scope"][at]["line"].as_u64();
    let start = line("start").unwrap_or(1);
    let end = match line("end") {
        Some(end) => end,
        None => line_count(file)?.saturating_add(1),
    };
    let middle = start + end.saturating_sub(start) / 2;
    if middle <= start {
        return None;
    }
    let mut row =
        octocode_runtime_base::response::result::fresh_query_row(ToolId::AstSearch, &query);
    let fields = row.as_object_mut()?;
    for field in ["include", "exclude", "maxDepth", "scope"] {
        fields.remove(field);
    }
    fields.insert("path".to_owned(), json!(display));
    let half = |from: u64, to: u64| {
        let mut row = row.clone();
        row["scope"] =
            json!({"start": {"line": from, "column": 1}, "end": {"line": to, "column": 1}});
        octocode_runtime_base::response::result::restart_query(ToolId::AstSearch, &row)
    };
    Some([half(start, middle), half(middle, end)])
}

/// The lines of `file`, counted in fixed-size reads: the file passed a limit
/// of its own, so it is never held whole. A final line without a newline
/// counts. `None` when it cannot be read.
fn line_count(file: &std::path::Path) -> Option<u64> {
    use std::io::Read;
    let (mut reader, _) = octocode_engine::text::open_regular(file).ok()?;
    let mut buffer = [0_u8; 16 * 1024];
    let mut count = octocode_engine::text::LineCount::default();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Some(octocode_engine::num::saturating_u64(count.lines())),
            Ok(read) => count.feed(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// `next` pages and the scan expansion.
pub(super) fn attach_match_continuations(
    q: MatchQuery<'_>,
    out: &mut Value,
    cut: ScanCut<'_>,
    more: bool,
    snapshot: &str,
) -> Result<(), ToolError> {
    let truncated = cut.truncated;
    let page = u64::from(q.page().max(1));
    // The scan continues after the files this window evaluated
    // (`scanOffset`), so the lead is offered once the window's last page is
    // reached.
    let expand_scan = super::super::scan_window::expand_scan(truncated, cut.resume)
        .map(|window| u64::from(window.scan_offset));
    // One stream of matches: the row's walk leads to its next page, then to
    // the next scan window. A continuation that cannot be built is a defect
    // of this response, not an absent lead: `hasMore` never stands without
    // its call.
    let with = |to: Move| {
        continuation_with(q, to, snapshot).ok_or_else(|| {
            ToolError::new(
                ErrorCode::OutputContractViolation,
                format!("astSearch could not build the continuation for {to:?}"),
            )
        })
    };
    if more {
        out["calls"] = octocode_runtime_base::response::collection::walk_calls(
            ToolId::AstSearch,
            None,
            with(Move::Page(page + 1))?,
        );
    }
    match expand_scan.filter(|_| !more) {
        // A window its page filled is the walk's next page: the same listing
        // goes on in the next window, counted as loaded until it ends.
        Some(offset) if cut.filled => {
            let next =
                continuation_with(q, Move::ScanWindow(offset), cut.pin).ok_or_else(|| {
                    ToolError::new(
                        ErrorCode::OutputContractViolation,
                        "astSearch could not build the next window's page",
                    )
                })?;
            out["calls"] = octocode_runtime_base::response::collection::walk_calls(
                ToolId::AstSearch,
                None,
                next,
            );
            super::super::scan_window::mark_resumed(out);
        }
        Some(offset) => {
            insert_expand_scan(q, out, Some(offset), cut.pin)?;
            super::super::scan_window::mark_resumed(out);
        }
        None => {}
    }
    Ok(())
}
/// `next.expandScan`: the window after the `offset` candidates a cut scan
/// evaluated, bound to `pin`, what those candidates are. A cut with no
/// position past them to resume at is a defect of the response.
pub(super) fn insert_expand_scan(
    q: MatchQuery<'_>,
    out: &mut Value,
    offset: Option<u64>,
    pin: &str,
) -> Result<(), ToolError> {
    let call = offset
        .and_then(|offset| continuation_with(q, Move::ScanWindow(offset), pin))
        .ok_or_else(|| {
            ToolError::new(
                ErrorCode::OutputContractViolation,
                "astSearch could not build the scan window's continuation",
            )
        })?;
    insert_next(&mut out["calls"], AstSearchChannel::ExpandScan, call);
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
    // Inline source has no file to lead to, and the caller holds its text:
    // the same `content` with operation syntaxTree prints its tree.
    if scope.inline.is_some() {
        return None;
    }
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
        path: Some(file.to_string_lossy().as_ref().try_into().ok()?),
        content: None,
        reasoning: None,
        snapshot: None,
    };
    Some(octocode_runtime_base::response::result::Continuation::related_query(tree).build())
}

/// The first line of `file` that spells a literal every match must contain,
/// as a whole word (`def` is not in `default`): where a parsed file's tree
/// is compared with the query's shape.
pub(super) fn anchor_line(q: MatchQuery<'_>, scope: &Scope, file: &std::path::Path) -> Option<u64> {
    let anchors = octocode_engine::structural::query_anchors(None, q.rule().as_ref())?;
    let source = scope.parse_source(file).ok().flatten()?;
    super::super::source::matched_text(file, &source)
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
