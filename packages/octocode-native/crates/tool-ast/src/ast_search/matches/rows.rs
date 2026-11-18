//! Match rows and the diagnostics a scan reports.
use super::query::{MatchQuery, Move};
use octocode_engine::structural::{StructuralDetailedMatch, StructuralDiagnostic};
use octocode_engine::text::analysis::{ObservedSpan, OriginProjection, SourceDescriptors};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::result::Continuation;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::response::source_evidence::{self, ProjectionBudget, ProjectionLimits};
use serde_json::{Value, json};
use std::sync::Arc;

/// One match row, an object: `line`, `column` (1-based), `endLine` for a
/// multi-line match, and `value` (source text or a structural header). With
/// `includeCaptures` the row also carries `endColumn` (exclusive), and each capture once in `metavarRanges` (1-based position, plus
/// `text` only when `value` does not hold it; the engine `metavars` map is a fallback for a capture
/// without a range).
pub(super) struct MatchRow {
    pub(super) value: Value,
    pub(super) source_descriptors: Option<Arc<SourceDescriptors>>,
    /// The lean row withheld something `includeCaptures:true` returns: a
    /// multi-line match cut to its header (signature) followed by `…`, or
    /// capture text the value does not show.
    pub(super) withheld: bool,
}

/// Whether the `inner` span (`(line, column)` start and end) lies inside
/// `outer`; positions order by line, then column.
fn within(outer: ((u32, u32), (u32, u32)), inner: ((u32, u32), (u32, u32))) -> bool {
    outer.0 <= inner.0 && inner.1 <= outer.1
}

fn display_value(m: &StructuralDetailedMatch, capture_text: bool) -> MatchRow {
    let header = m.header.as_deref().filter(|_| !capture_text);
    let text = match header {
        Some(header) => format!("{header} …"),
        None => m.text.clone(),
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
            "column": octocode_runtime_base::num::one_based_column(m.start_col),
        });
        if m.end_line != m.start_line {
            value["endLine"] = json!(m.end_line);
        }
        value["value"] = json!(text);
        if header.is_some() {
            value["clipped"] = json!(true);
        }
        return MatchRow {
            source_descriptors: None,
            value,
            withheld,
        };
    }
    use octocode_runtime_base::num::one_based_column;
    // Every expanded row states its end, so `line`, `column`, `endLine`
    // (else `line`) and `endColumn` are the `rule.range` that selects it.
    let mut value =
        json!({"line":m.start_line,"value":text,"column":one_based_column(m.start_col)});
    if m.end_line != m.start_line {
        value["endLine"] = json!(m.end_line);
    }
    value["endColumn"] = json!(one_based_column(m.end_col));
    let mut ranges = serde_json::Map::new();
    for (name, values) in &m.metavar_ranges {
        let rows = values
            .iter()
            .map(|range| {
                let mut row = json!({
                    "line":range.line,
                    "column":one_based_column(range.column),
                    "endColumn":one_based_column(range.end_column)
                });
                // `value` is the match's verbatim source here (a header or
                // clipped value is a lean row, which has no ranges): a range
                // inside the span states the text, so only a capture outside
                // it keeps `text`.
                if !within(
                    ((m.start_line, m.start_col), (m.end_line, m.end_col)),
                    (
                        (range.line, range.column),
                        (range.end_line, range.end_column),
                    ),
                ) {
                    row["text"] = json!(range.text);
                }
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
        source_descriptors: None,
        value,
        withheld: false,
    }
}

/// One match's row; byte ranges, origins and digests only on request
/// (`includeProvenance`).
pub(super) fn match_value(
    matched: &StructuralDetailedMatch,
    capture_text: bool,
    provenance: bool,
    cancel: &dyn CancellationCheck,
) -> Result<MatchRow, ToolError> {
    if !provenance {
        return Ok(display_value(matched, capture_text));
    }
    let descriptors = matched
        .source_descriptors
        .as_ref()
        .ok_or_else(missing_origin)?;
    let mut budget = ProjectionBudget::new(ProjectionLimits::default());
    let mut row = display_value(matched, capture_text);
    put_span(
        &mut row.value,
        matched.source_span,
        matched.source_origin.as_ref(),
        descriptors,
        ("sourceRange", "sourceOrigin"),
        true,
        &mut budget,
        cancel,
    )?;
    if !capture_text && matched.header.is_some() {
        let span = matched.header_span.ok_or_else(missing_origin)?;
        put_span(
            &mut row.value,
            span,
            matched.header_origin.as_ref(),
            descriptors,
            ("headerSourceRange", "headerOrigin"),
            false,
            &mut budget,
            cancel,
        )?;
    }
    if capture_text {
        for (name, captures) in &matched.metavar_ranges {
            for (index, capture) in captures.iter().enumerate() {
                put_span(
                    &mut row.value["metavarRanges"][name][index],
                    capture.source_span,
                    capture.source_origin.as_ref(),
                    descriptors,
                    ("sourceRange", "sourceOrigin"),
                    true,
                    &mut budget,
                    cancel,
                )?;
            }
        }
    }
    row.source_descriptors = Some(Arc::clone(descriptors));
    Ok(row)
}

fn missing_origin() -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::ExecutionFailed,
        "Structural evidence was not bound to its admitted source bytes.",
    )
}

fn put_span(
    row: &mut Value,
    span: ObservedSpan,
    origin: Option<&OriginProjection>,
    descriptors: &SourceDescriptors,
    keys: (&str, &str),
    digest: bool,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    let origin = origin.ok_or_else(missing_origin)?;
    let projected = source_evidence::project_origin(origin, budget, cancel)?;
    let encode = |error: serde_json::Error| {
        ToolError::new(
            octocode_contract_types::ErrorCode::ExecutionFailed,
            error.to_string(),
        )
    };
    row[keys.0] =
        serde_json::to_value(source_evidence::project_range(span.range)?).map_err(encode)?;
    row[keys.1] = serde_json::to_value(projected).map_err(encode)?;
    if digest {
        row["sourceDigest"] = json!(source_evidence::source_digest(
            descriptors,
            &origin.analysis_unit_id
        )?);
    }
    Ok(())
}

/// `q` moved to `to`, as a continuation bound to `snapshot`.
pub(super) fn continuation_with(
    q: MatchQuery<'_>,
    to: Move,
    snapshot: &str,
) -> Option<Continuation> {
    Some(Continuation::continued_query(q.moved(to, snapshot)?))
}

pub(super) fn diag(d: &StructuralDiagnostic) -> Value {
    octocode_tools_shared::ast_rule::diagnostic_row(d, d.path.as_deref())
}

/// `diagnostic` added to a rendered row's diagnostics.
pub(super) fn push_diagnostic(out: &mut Value, diagnostic: Value) {
    match out["diagnostics"].as_array_mut() {
        Some(diagnostics) => diagnostics.push(diagnostic),
        None => out["diagnostics"] = json!([diagnostic]),
    }
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
mod tests;
