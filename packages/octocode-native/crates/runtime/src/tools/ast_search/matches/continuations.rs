//! A match page's continuations: its next page and scan window, the value
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
    let mut row = crate::response::result::fresh_query_row(ToolId::AstSearch, &query);
    let fields = row.as_object_mut()?;
    for field in ["include", "exclude", "maxDepth", "scope"] {
        fields.remove(field);
    }
    fields.insert("path".to_owned(), json!(display));
    let half = |from: u64, to: u64| {
        let mut row = row.clone();
        row["scope"] =
            json!({"start": {"line": from, "column": 1}, "end": {"line": to, "column": 1}});
        crate::response::result::restart_query(ToolId::AstSearch, &row)
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
    let (mut lines, mut last) = (0_u64, None);
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Some(lines + u64::from(last.is_some_and(|byte| byte != b'\n'))),
            Ok(read) => {
                let read = &buffer[..read];
                let newlines = read.iter().filter(|byte| **byte == b'\n').count();
                lines = lines.saturating_add(octocode_engine::num::saturating_u64(newlines));
                last = read.last().copied();
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// `next` pages and the scan expansion; the related capture expansion and
/// read of the top file's hits.
pub(super) fn attach_match_continuations(
    q: MatchQuery<'_>,
    scope: &Scope,
    out: &mut Value,
    page_groups: &[Group],
    cut: ScanCut<'_>,
    more: bool,
    snapshot: &str,
) -> Result<(), ToolError> {
    let truncated = cut.truncated;
    let page = u64::from(q.page().max(1));
    let has_truncated_captures = page_groups.iter().any(|group| group.truncated_captures);
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
        out["calls"] = json!({});
        insert_next(
            &mut out["calls"],
            AstSearchChannel::NextPage,
            with(Move::Page(page + 1))?,
        );
    }
    if let Some(offset) = expand_scan.filter(|_| !more) {
        insert_expand_scan(q, out, Some(offset), cut.pin)?;
        super::super::scan_window::mark_resumed(out);
    }
    // The lean structural representation may withhold a body or captures.
    // Expanding that chosen representation returns their complete text:
    // optional work, so a related lead, never a required continuation.
    if has_truncated_captures && !q.capture_text().unwrap_or(false) {
        let (lead, whole_scope) = capture_lead(q, scope, page_groups);
        out["calls"]["expandCaptures"] = lead;
        if whole_scope {
            crate::response::result::add_hints(
                out,
                [
                    "The includeCaptures lead reads the whole scope: this page's withheld rows are in more files than include holds, or in a name no glob spells. It returns every match of the scope, those rows among them.",
                ],
            );
        }
    }
    if let Some(read) = page_groups
        .first()
        .and_then(|top| read_top_hits(scope, &top.row))
    {
        out["calls"]["read"] = read;
    }
    Ok(())
}
/// The query again with `includeCaptures`, over only the files whose rows
/// this page withheld text of: their matches with captures cut into pages of
/// their own (captures change a row's size, so this page's edges would not
/// hold), so walking it lists each withheld row once, whole, and reads no
/// other file of the scope. Each file is an `include` glob anchored at the
/// scope with its name escaped; a single-file scope is that file already,
/// and a name no glob can spell (not UTF-8), or more files than `include`
/// holds, keeps the whole scope, a superset that still lists each row once;
/// the flag says it did, so the page discloses it.
fn capture_lead(q: MatchQuery<'_>, scope: &Scope, page_groups: &[Group]) -> (Value, bool) {
    let mut changes =
        json!({"includeCaptures": true, "page": null, "scanOffset": null, "snapshot": null});
    let anchored = |file: std::path::PathBuf| {
        let below = file.strip_prefix(&scope.canonical).ok()?;
        let parts = below
            .components()
            .map(|part| {
                part.as_os_str()
                    .to_str()
                    .map(octocode_engine::search::glob::literal)
            })
            .collect::<Option<Vec<_>>>()?;
        Some(format!("/{}", parts.join("/")))
    };
    let files = page_groups
        .iter()
        .filter(|group| group.truncated_captures)
        .map(|group| super::reads::row_file(scope, &group.row).and_then(anchored))
        .collect::<Option<Vec<_>>>();
    let fits = |files: &Vec<String>| {
        files.len() <= crate::contracts::query_limits::ast_search::r#match::INCLUDE_MAX_ITEMS
    };
    let files = files.filter(|files| !scope.is_file && fits(files));
    let whole_scope = files.is_none() && !scope.is_file;
    if let Some(files) = files {
        changes["include"] = json!(files);
    }
    let lead = crate::response::result::restart_query(
        ToolId::AstSearch,
        &crate::response::result::with_changes(q.to_value(), &changes),
    );
    (lead, whole_scope)
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
