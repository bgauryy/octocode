//! Observed diagnostic health, separate from server readiness and binding.
use super::failure::mark_anchor_line_errors;
use super::lsp_search_query;
use octocode_contract_types::{LspDocumentOperation, LspSearchQuery};
use serde_json::{Value, json};

pub(super) fn annotate_diagnostics(
    row: &mut Value,
    query: &LspSearchQuery,
    report: Option<&Value>,
    anchor_line: u32,
) {
    if !row.is_object() || row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    let Some(items) = report else {
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
            let start = item.get("line").and_then(Value::as_u64);
            let end = super::locations::place_last_line(item);
            matches!((start, end), (Some(start), Some(end)) if start <= u64::from(anchor_line) && u64::from(anchor_line) <= end)
        })
        .count();
    let assessment = context(row);
    assessment["diagnostics"] = json!(if errors.is_empty() {
        "noErrorsReported"
    } else {
        "errors"
    });
    assessment["diagnosticsComplete"] = json!(true);
    assessment["errorCount"] = json!(errors.len());
    assessment["anchorErrorCount"] = json!(anchor_errors);
    if errors.is_empty() {
        return;
    }
    mark_anchor_line_errors(row, items, anchor_line);
    if anchor_errors == 0 || row.get("status").and_then(Value::as_str) != Some("empty") {
        octocode_runtime_base::response::paging::mark_partial(
            row,
            octocode_runtime_base::response::paging::PartialReason::DocumentAnalysisErrors,
            &[format!(
                "The language server reported {} in this document. Errors may affect analysis beyond their own lines; this result does not establish complete semantic coverage.",
                octocode_engine::text::counted(
                    errors.len(),
                    "error diagnostic",
                    "error diagnostics"
                )
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
    let Some(diagnostic) =
        lsp_search_query::document_lead(LspDocumentOperation::Diagnostic, path, Some(query))
    else {
        return;
    };
    octocode_runtime_base::response::result::attach_related(
        row,
        octocode_runtime_base::response::result::Continuation::related_query(diagnostic).build(),
    );
}

#[cfg(test)]
mod tests;
