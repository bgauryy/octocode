//! Interpreter for the `lsp_rust_context` opcode, which also refuses
//! `view:"files"` with `contextLines` (grouped rows list lines only).
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
                    vec![
                        "queries".into(),
                        index.to_string(),
                        "rustContext".into(),
                        "procMacros".into(),
                    ],
                    "procMacros requires buildScripts:true because rust-analyzer builds procedural macros through Cargo.",
                ));
            }
            if !query
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| path.to_lowercase().ends_with(".rs"))
            {
                return Err(issue(
                    "lsp.rust-path",
                    prefix("rustContext"),
                    "rustContext requires a Rust .rs path, including for workspaceSymbol.",
                ));
            }
        }
    }
    Ok(())
}
