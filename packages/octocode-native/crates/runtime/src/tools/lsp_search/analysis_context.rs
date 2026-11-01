//! Observed diagnostic health, separate from server readiness and binding.
use super::failure::mark_anchor_line_errors;
use super::lsp_search_query;
use crate::id::ToolId;
use octocode_contract_types::LspSearchQuery;
use serde_json::{Value, json};

pub(super) fn annotate_diagnostics(
    row: &mut Value,
    query: &LspSearchQuery,
    report: Option<(&Value, bool)>,
    anchor_line: u32,
) {
    if !row.is_object() || row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    let Some((items, truncated)) = report else {
        context(row)["diagnostics"] = json!("unavailable");
        return;
    };
    let errors = super::render::items(items)
        .iter()
        .filter(|item| item.get("severity").and_then(Value::as_str) == Some("error"))
        .collect::<Vec<_>>();
    let anchor_errors = errors
        .iter()
        .filter(|item| {
            let start = item.pointer("/displayRange/startLine").and_then(Value::as_u64);
            let end = item.pointer("/displayRange/endLine").and_then(Value::as_u64);
            matches!((start, end), (Some(start), Some(end)) if start <= u64::from(anchor_line) && u64::from(anchor_line) <= end)
        })
        .count();
    let assessment = context(row);
    assessment["diagnostics"] = json!(if errors.is_empty() {
        "noErrorsReported"
    } else {
        "errors"
    });
    assessment["diagnosticsComplete"] = json!(!truncated);
    assessment["errorCount"] = json!(errors.len());
    assessment["anchorErrorCount"] = json!(anchor_errors);
    if truncated {
        crate::response::paging::mark_partial(
            row,
            crate::response::paging::PartialReason::DiagnosticsTruncated,
            &[],
        );
    }
    if errors.is_empty() {
        return;
    }
    mark_anchor_line_errors(row, items, anchor_line);
    if anchor_errors == 0 || row.get("status").and_then(Value::as_str) != Some("empty") {
        crate::response::paging::mark_partial(
            row,
            crate::response::paging::PartialReason::DocumentAnalysisErrors,
            &[format!(
                "The language server reported {} error diagnostics in this document. They may affect analysis beyond their own lines; this result does not establish complete semantic coverage.",
                errors.len()
            )],
        );
    }
    attach_diagnostics(row, query);
}

pub(super) fn context(row: &mut Value) -> &mut Value {
    if !row["lsp"].is_object() {
        row["lsp"] = json!({});
    }
    if !row["lsp"]["analysisContext"].is_object() {
        row["lsp"]["analysisContext"] = json!({});
    }
    &mut row["lsp"]["analysisContext"]
}

fn attach_diagnostics(row: &mut Value, query: &LspSearchQuery) {
    let Some(path) = lsp_search_query::path(query) else {
        return;
    };
    let mut diagnostic = json!({"operation":"diagnostic", "path":path});
    let original = lsp_search_query::to_row(query);
    for field in ["workspaceRoot", "rustContext"] {
        if let Some(value) = original.get(field).filter(|value| !value.is_null()) {
            diagnostic[field] = value.clone();
        }
    }
    crate::response::result::attach_related(
        row,
        crate::response::result::Continuation::related(ToolId::LspSearch, diagnostic).build(),
    );
}

#[cfg(test)]
mod tests;
