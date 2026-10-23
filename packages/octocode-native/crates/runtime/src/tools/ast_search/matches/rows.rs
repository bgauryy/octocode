//! Match rows and the diagnostics a scan reports.
use super::query::MatchQuery;
use crate::id::ToolId;
use crate::response::result::ToolError;
use octocode_engine::structural::{StructuralDetailedMatch, StructuralDiagnostic};
use serde_json::{Value, json};

/// One match row, an object: `line`, `column` (1-based), `endLine` for a
/// multi-line match, and `value` (source text or a marked, bounded prefix). With
/// `captureText` the row also carries `endColumn` for a multi-line span, and each capture once in `metavarRanges` (text plus
/// 1-based position; the engine `metavars` map is a fallback for a capture
/// without a range).
pub(super) struct MatchRow {
    pub(super) value: Value,
    /// The lean row withheld something `captureText:true` returns: a
    /// multi-line match cut to its header (signature) followed by `…`, or
    /// capture text the value does not show.
    pub(super) withheld: bool,
    /// The shown text was cut at `matchContentLength`.
    pub(super) shown_cut: bool,
    /// The match's 1-based start and end lines.
    pub(super) lines: (u64, u64),
    /// The whole value's characters when they exceed `matchContentLength`,
    /// so the shown or the `captureText` value is cut.
    pub(super) cut: Option<usize>,
}

pub(super) fn match_value(
    m: &StructuralDetailedMatch,
    capture_text: bool,
    content_length: usize,
) -> MatchRow {
    let lines = (u64::from(m.start_line), u64::from(m.end_line));
    let whole_chars = m.text.chars().count();
    let cut = (whole_chars > content_length).then_some(whole_chars);
    let header = m.header.as_deref().filter(|_| !capture_text);
    let (text, shown_cut) = match header {
        Some(header) => bounded_match(&format!("{header} …"), content_length),
        None => bounded_match(&m.text, content_length),
    };
    if !capture_text {
        let hidden = |capture: &str| !capture.is_empty() && !text.contains(capture);
        let withheld = header.is_some()
            || m.metavar_ranges
                .values()
                .flatten()
                .any(|range| hidden(&range.text))
            || m.metavars.values().flatten().any(|value| hidden(value));
        // A row is an object named like the inputs that take it; a
        // multi-line match states its last line.
        let mut value = json!({
            "line": m.start_line,
            "column": crate::num::one_based_column(m.start_col),
        });
        if m.end_line != m.start_line {
            value["endLine"] = json!(m.end_line);
        }
        value["value"] = json!(text);
        if header.is_some() || shown_cut {
            value["clipped"] = json!(true);
        }
        return MatchRow {
            value,
            withheld,
            shown_cut,
            lines,
            cut,
        };
    }
    use crate::num::one_based_column;
    let mut value =
        json!({"line":m.start_line,"value":text,"column":one_based_column(m.start_col)});
    if shown_cut {
        value["clipped"] = json!(true);
    }
    if m.end_line != m.start_line {
        value["endLine"] = json!(m.end_line);
        value["endColumn"] = json!(one_based_column(m.end_col));
    }
    let mut ranges = serde_json::Map::new();
    for (name, values) in &m.metavar_ranges {
        let rows = values
            .iter()
            .map(|range| {
                let mut row = json!({
                    "text":range.text,
                    "line":range.line,
                    "column":one_based_column(range.column),
                    "endColumn":one_based_column(range.end_column)
                });
                if range.end_line != range.line {
                    row["endLine"] = json!(range.end_line);
                }
                row
            })
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            ranges.insert(name.clone(), Value::Array(rows));
        }
    }
    // Capture maps are unordered; emit names sorted so output is deterministic.
    let mut captures: Vec<_> = m.metavars.iter().collect();
    captures.sort_by(|a, b| a.0.cmp(b.0));
    let mut metavars = serde_json::Map::new();
    for (name, values) in captures {
        if !ranges.contains_key(name) && !values.is_empty() {
            metavars.insert(name.clone(), json!(values));
        }
    }
    if !metavars.is_empty() {
        value["metavars"] = Value::Object(metavars);
    }
    if !ranges.is_empty() {
        value["metavarRanges"] = Value::Object(ranges);
    }
    MatchRow {
        value,
        withheld: false,
        shown_cut,
        lines,
        cut,
    }
}

/// Exact source text bounded to `limit` Unicode scalars
/// (`matchContentLength`), including the trailing ellipsis when cut, and
/// whether it was cut.
pub(super) fn bounded_match(text: &str, limit: usize) -> (String, bool) {
    if text.chars().count() > limit {
        let mut output = text
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        output.push('…');
        (output, true)
    } else {
        (text.to_owned(), false)
    }
}

pub(super) fn match_display_path(root: &std::path::Path, path: &str) -> String {
    let candidate = std::path::Path::new(path);
    let relative = candidate.strip_prefix(root).unwrap_or(candidate);
    super::super::rooted_display(root, relative)
}

pub(super) fn continuation_with(q: MatchQuery<'_>, changes: Value, snapshot: &str) -> Value {
    let mut query = q.to_value();
    query["snapshot"] = json!(snapshot);
    let query = crate::response::result::with_changes(query, &changes);
    crate::response::result::Continuation::new(ToolId::AstSearch, query).build()
}

pub(super) fn diag(d: &StructuralDiagnostic) -> Value {
    crate::tools::ast_rule::diagnostic_row(d, d.path.as_deref())
}

pub(super) fn error_diagnostic(
    diagnostics: &[StructuralDiagnostic],
) -> Option<&StructuralDiagnostic> {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == "error")
}

/// The engine diagnostic as a tool error carrying its declared wire code.
pub(super) fn diagnostic_error(diagnostic: &StructuralDiagnostic) -> ToolError {
    ToolError::new(
        super::super::errors::engine_error_code(&diagnostic.code),
        diagnostic.message.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(code: &str) -> StructuralDiagnostic {
        StructuralDiagnostic {
            code: code.to_owned(),
            severity: "error".to_owned(),
            stage: "search".to_owned(),
            message: "failed".to_owned(),
            path: None,
            recovery: None,
        }
    }

    #[test]
    fn diagnostic_errors_carry_only_declared_error_codes() {
        for (engine, wire) in [
            (octocode_engine::structural::RULE_INVALID, "invalidPattern"),
            ("structural.file.unreadable", "fileReadFailed"),
            (octocode_engine::structural::MATCH_DEADLINE, "timeout"),
            ("structural.unknown.failure", "executionFailed"),
        ] {
            assert_eq!(
                crate::id::error_codes::as_str(diagnostic_error(&diagnostic(engine)).code),
                wire,
                "{engine}"
            );
        }
    }
}
