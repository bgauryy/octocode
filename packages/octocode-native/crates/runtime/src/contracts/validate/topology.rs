//! Interpreter for the `ast_topology` opcode.
use super::{ContractValidationError, issue, query_values};
use serde_json::Value;

pub(super) fn validate_topology_queries(input: &Value) -> Result<(), ContractValidationError> {
    for (index, query) in query_values(input) {
        let operation = query.get("operation").and_then(Value::as_str);
        if !matches!(
            operation,
            Some("dependencies" | "dependents" | "path" | "reachability" | "cycles" | "deadCode")
        ) {
            continue;
        }
        let absolute = |value: Option<&Value>| {
            value.and_then(Value::as_str).is_some_and(|s| {
                s.starts_with('/')
                    || s.starts_with("\\\\")
                    || (s.len() > 2
                        && s.as_bytes()[1] == b':'
                        && matches!(s.as_bytes()[2], b'/' | b'\\'))
            })
        };
        let rooted = query
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
            || absolute(query.get("source"))
            || absolute(query.get("target"))
            || query
                .get("entrypoints")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|v| absolute(Some(v))));
        if !rooted {
            return Err(issue(
                "ast-search.topology",
                vec!["queries".into(), index.to_string(), "path".into()],
                "path is required unless an absolute path can infer the repository root",
            ));
        }
    }
    Ok(())
}
