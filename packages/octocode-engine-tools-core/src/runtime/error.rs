use super::RuntimeError;
use serde_json::{Value, json};

/// Project a typed contract failure into the CallToolResult shape returned by
/// the frozen MCP SDK registration layer. Runtime and transport faults remain
/// errors at the NAPI boundary.
pub fn mcp_input_error(tool: &str, input: &Value, error: &RuntimeError) -> Option<Value> {
    (error.code == "invalidInput").then(|| {
        let detail = envelope_detail(input).unwrap_or_else(|| {
            error.validation_issues.as_deref().map_or_else(
                || error.message.clone(),
                render_issues,
            )
        });
        json!({
            "content": [{
                "type": "text",
                "text": format!("Input validation error: Invalid arguments for tool {tool}: {detail}")
            }],
            "isError": true
        })
    })
}

pub fn mcp_envelope_error(tool: &str, input: &Value) -> Option<Value> {
    envelope_detail(input).map(|detail| result(tool, &detail))
}

fn result(tool: &str, detail: &str) -> Value {
    json!({
        "content": [{"type":"text","text":format!(
            "Input validation error: Invalid arguments for tool {tool}: {detail}"
        )}],
        "isError": true
    })
}

fn envelope_detail(input: &Value) -> Option<String> {
    if !input.is_object() {
        return Some(format!(
            "Invalid input: expected object, received {}",
            json_type(input)
        ));
    }
    let Some(queries) = input.get("queries") else {
        return Some("queries: Invalid input: expected array, received undefined".into());
    };
    if !queries.is_array() {
        return Some(format!(
            "queries: Invalid input: expected array, received {}",
            json_type(queries)
        ));
    }
    None
}

fn render_issue(issue: &crate::contracts::ValidationIssue) -> String {
    if issue.rule_id == "schema.union-selected-keys" {
        return format!("{}: input: {}", issue.path.join("."), issue.message);
    }
    // The canonical range refinement is inside the selector union; MCP's
    // union formatter retains that boundary while the direct CLI flattens it.
    if issue.rule_id == "content.range-order" {
        let parent = issue.path[..issue.path.len().saturating_sub(1)].join(".");
        return format!("{parent}: endLine: {}", issue.message);
    }
    let path = issue.path.join(".");
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}: ")
    };
    if issue.rule_id == "schema.required"
        && let Some(field) = issue.path.last()
    {
        let parent = issue.path[..issue.path.len().saturating_sub(1)].join(".");
        let expected = issue
            .schema
            .as_ref()
            .and_then(|v| v["type"].as_str())
            .unwrap_or("nonoptional");
        return format!(
            "{parent}: {field}: Invalid input: expected {expected}, received undefined"
        );
    }
    if issue.rule_id == "schema.type" {
        let expected = issue
            .schema
            .as_ref()
            .and_then(|v| v["type"].as_str())
            .unwrap_or("value");
        let received = issue
            .received
            .as_ref()
            .map(json_type)
            .unwrap_or("undefined");
        return format!("{prefix}Invalid input: expected {expected}, received {received}");
    }
    if issue.rule_id == "schema.enum"
        && let Some(values) = issue.schema.as_ref().and_then(|v| v["enum"].as_array())
    {
        let choices = values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("|");
        return format!("{prefix}Invalid option: expected one of {choices}");
    }
    format!("{prefix}{}", issue.message)
}

fn render_issues(issues: &[crate::contracts::ValidationIssue]) -> String {
    let mut rendered = Vec::new();
    let mut consumed = vec![false; issues.len()];
    for (index, issue) in issues.iter().enumerate() {
        if consumed[index] {
            continue;
        }
        if issue.rule_id == "schema.unknown-field" {
            let parent = &issue.path[..issue.path.len().saturating_sub(1)];
            let mut fields = Vec::new();
            for (candidate_index, candidate) in issues.iter().enumerate().skip(index) {
                if candidate.rule_id == "schema.unknown-field"
                    && candidate.path[..candidate.path.len().saturating_sub(1)] == *parent
                {
                    consumed[candidate_index] = true;
                    if let Some(field) = candidate.path.last() {
                        fields.push(format!("\"{field}\""));
                    }
                }
            }
            rendered.push(format!(
                "{}: Unrecognized key{}: {}",
                parent.join("."),
                if fields.len() == 1 { "" } else { "s" },
                fields.join(", ")
            ));
        } else {
            consumed[index] = true;
            rendered.push(render_issue(issue));
        }
    }
    rendered.join(", ")
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

    fn error(issues: Vec<crate::contracts::ValidationIssue>) -> RuntimeError {
        RuntimeError {
            code: "invalidInput".into(),
            message: "contract validation failed".into(),
            payload: None,
            validation_issues: Some(issues),
        }
    }

    fn issue(rule: &str, path: &[&str], message: &str) -> crate::contracts::ValidationIssue {
        crate::contracts::ValidationIssue {
            rule_id: rule.into(),
            path: path.iter().map(|part| (*part).into()).collect(),
            message: message.into(),
            schema: (rule == "schema.required").then(|| json!({"type":"string"})),
            received: None,
        }
    }

    #[test]
    fn matches_frozen_missing_wrong_type_and_unknown_field_messages() {
        let cases = [
            (
                json!({}),
                error(vec![]),
                "queries: Invalid input: expected array, received undefined",
            ),
            (
                json!({"queries":"bad"}),
                error(vec![]),
                "queries: Invalid input: expected array, received string",
            ),
            (
                json!({"queries":[{}]}),
                error(vec![issue(
                    "schema.required",
                    &["queries", "0", "path"],
                    "missing",
                )]),
                "queries.0: path: Invalid input: expected string, received undefined",
            ),
            (
                json!({"queries":[{"path":"x","wat":true}]}),
                error(vec![issue(
                    "schema.unknown-field",
                    &["queries", "0", "wat"],
                    "unknown",
                )]),
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
