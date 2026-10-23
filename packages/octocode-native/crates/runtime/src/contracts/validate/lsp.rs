//! Interpreter for the `lsp_rust_context` opcode, which also refuses
//! `groupByFile` with `contextLines` (grouped rows list lines only).
use super::{ContractValidationError, issue, query_values};
use serde_json::Value;

pub(super) fn validate_lsp_queries(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let prefix = |field: &str| vec!["queries".into(), index.to_string(), field.into()];
        if query.get("groupByFile") == Some(&Value::Bool(true))
            && query.get("contextLines").is_some()
        {
            return Err(issue(
                "lsp.group-context",
                prefix("contextLines"),
                "groupByFile lists lines only; drop contextLines or groupByFile",
            ));
        }
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

#[cfg(test)]
mod tests {
    use crate::contracts::validate::validate;
    use serde_json::json;

    /// groupByFile lists lines only, so context it would drop is refused.
    #[test]
    fn grouped_locations_refuse_source_context() {
        let anchor = json!({"path":"a.ts","symbolName":"x","line":1});
        for operation in ["references", "definition"] {
            let mut query = anchor.clone();
            query["operation"] = json!(operation);
            query["groupByFile"] = json!(true);
            query["contextLines"] = json!(2);
            let error = validate("lspSearch", json!({"queries":[query]}))
                .expect_err("grouped rows carry no context");
            assert_eq!(error.issues[0].path, ["queries", "0", "contextLines"]);
            assert_eq!(
                error.issues[0].message,
                "groupByFile lists lines only; drop contextLines or groupByFile"
            );
        }
        for control in [json!({"groupByFile":true}), json!({"contextLines":2})] {
            let mut query = anchor.clone();
            query
                .as_object_mut()
                .expect("row")
                .extend(control.as_object().expect("control").clone());
            validate("lspSearch", json!({"queries":[query]})).expect("one control alone");
        }
    }
}
