//! Interpreter for the `local_search_mode` opcode.
use super::{ContractValidationError, issue, query_values};
use serde_json::Value;

pub(super) fn validate_local_search_queries(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let prefix = |field: &str| vec!["queries".into(), index.to_string(), field.into()];
        if query
            .get("matchString")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(issue(
                "local-search.match-string",
                prefix("matchString"),
                "matchString is required",
            ));
        }
        let is_match_only = query.get("resultView").and_then(Value::as_str) == Some("matchOnly");
        if let Some(unique @ ("list" | "count")) = query.get("unique").and_then(Value::as_str)
            && !is_match_only
        {
            return Err(issue(
                "local-search.unique",
                prefix("unique"),
                format!("unique:\"{unique}\" requires resultView:\"matchOnly\""),
            ));
        }
    }
    Ok(())
}
