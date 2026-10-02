//! Interpreter for the `lsp_rust_context` opcode.
use super::{ContractValidationError, issue, query_values};
use serde_json::Value;

pub(super) fn validate_lsp_queries(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let prefix = |field: &str| vec!["queries".into(), index.to_string(), field.into()];
        if let Some(context) = query.get("rustContext").and_then(Value::as_object) {
            if context.get("procMacros") == Some(&Value::Bool(true))
                && context.get("buildScripts") != Some(&Value::Bool(true))
            {
                return Err(issue(
                    "lsp.proc-macros",
                    prefix("rustContext"),
                    "procMacros requires buildScripts:true",
                ));
            }
            if !query
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| uri.to_lowercase().ends_with(".rs"))
            {
                return Err(issue(
                    "lsp.rust-uri",
                    prefix("rustContext"),
                    "rustContext requires a Rust .rs uri",
                ));
            }
        }
        let operation = query.get("operation").and_then(Value::as_str).unwrap_or("");
        if operation == "workspaceSymbol" {
            if query.get("position").is_some() || query.get("lineHint").is_some() {
                return Err(issue(
                    "lsp.workspace-anchor",
                    prefix("position"),
                    "workspaceSymbol does not accept a position or lineHint",
                ));
            }
            if query
                .get("symbolName")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(issue(
                    "lsp.workspace-symbol",
                    prefix("symbolName"),
                    "Set symbolName for workspaceSymbol",
                ));
            }
            if query.get("uri").is_none() && query.get("workspaceRoot").is_none() {
                return Err(issue(
                    "lsp.workspace-root",
                    prefix("workspaceRoot"),
                    "Set uri or workspaceRoot for workspaceSymbol",
                ));
            }
            continue;
        }
        if query.get("uri").is_none() {
            return Err(issue(
                "lsp.uri",
                prefix("uri"),
                "Set uri for file-scoped operations",
            ));
        }
        if matches!(operation, "documentSymbols" | "diagnostic") {
            continue;
        }
        let has_name = query
            .get("symbolName")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty())
            || query.get("lineHint").is_some();
        let has_position = query.get("position").is_some();
        if has_name && has_position {
            return Err(issue(
                "lsp.anchor-exclusive",
                prefix("position"),
                "Use either symbolName+lineHint or position, not both",
            ));
        }
        if !has_position
            && query
                .get("symbolName")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err(issue(
                "lsp.symbol-anchor",
                prefix("symbolName"),
                "Set symbolName for anchored operations",
            ));
        }
        if !has_position && query.get("lineHint").and_then(Value::as_i64).is_none() {
            return Err(issue(
                "lsp.line-anchor",
                prefix("lineHint"),
                "Set lineHint for anchored operations",
            ));
        }
    }
    Ok(())
}
