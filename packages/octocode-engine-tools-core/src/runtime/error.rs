use super::RuntimeError;
use serde_json::{Value, json};

/// Project a typed contract failure into the CallToolResult shape returned by
/// the frozen MCP SDK registration layer. Runtime and transport faults remain
/// errors at the NAPI boundary.
pub fn mcp_input_error(tool: &str, input: &Value, error: &RuntimeError) -> Option<Value> {
    (error.code == "invalidInput").then(|| {
        let detail = frozen_validation_detail(input, error);
        json!({
            "content": [{
                "type": "text",
                "text": format!("Input validation error: Invalid arguments for tool {tool}: {detail}")
            }],
            "isError": true
        })
    })
}

fn frozen_validation_detail(input: &Value, error: &RuntimeError) -> String {
    let Some(queries) = input.get("queries") else {
        return "queries: Invalid input: expected array, received undefined".into();
    };
    if !queries.is_array() {
        return format!(
            "queries: Invalid input: expected array, received {}",
            json_type(queries)
        );
    }
    let details = error
        .payload
        .as_ref()
        .and_then(|payload| payload.get("details"))
        .and_then(Value::as_array)
        .map(|details| details.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(detail) = details.first() {
        if let Some((location, field)) = detail.split_once(": Missing required field: ") {
            let parent = location
                .rsplit_once('.')
                .map_or(location, |(parent, _)| parent);
            return format!(
                "{parent}: {field}: Invalid input: expected string, received undefined"
            );
        }
        if let Some(fields) = detail.strip_prefix("Remove unknown field(s) from query ")
            && let Some((one_based, fields)) = fields.split_once(": ")
            && let Ok(index) = one_based.parse::<usize>()
        {
            let quoted = fields
                .split(", ")
                .map(|field| format!("\"{field}\""))
                .collect::<Vec<_>>()
                .join(", ");
            return format!("queries.{}: Unrecognized key: {quoted}", index - 1);
        }
        return detail.to_string();
    }
    error.message.clone()
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(details: &[&str]) -> RuntimeError {
        RuntimeError {
            code: "invalidInput".into(),
            message: "contract validation failed".into(),
            payload: Some(json!({ "details": details })),
        }
    }

    #[test]
    fn matches_frozen_missing_wrong_type_and_unknown_field_messages() {
        let cases = [
            (
                json!({}),
                error(&["queries.0.path: Missing required field: path"]),
                "queries: Invalid input: expected array, received undefined",
            ),
            (
                json!({"queries":"bad"}),
                error(&["queries must be an array"]),
                "queries: Invalid input: expected array, received string",
            ),
            (
                json!({"queries":[{}]}),
                error(&["queries.0.path: Missing required field: path"]),
                "queries.0: path: Invalid input: expected string, received undefined",
            ),
            (
                json!({"queries":[{"path":"x","wat":true}]}),
                error(&["Remove unknown field(s) from query 1: wat"]),
                "queries.0: Unrecognized key: \"wat\"",
            ),
        ];
        for (input, error, expected) in cases {
            let result = mcp_input_error("localFetch", &input, &error).unwrap();
            assert_eq!(
                result["content"][0]["text"],
                format!(
                    "Input validation error: Invalid arguments for tool localFetch: {expected}"
                )
            );
            assert_eq!(result["isError"], true);
            assert!(result.get("structuredContent").is_none());
        }
    }
}
