use super::{ContractValidationError, ValidationIssue, query_label, sibling_requirement};
use serde_json::Value;

/// Projects transport-neutral validation issues into the stable tool-error
/// envelope. `mcp` selects the schema pointer: MCP clients read the tool's
/// `inputSchema`; only the CLI has the `schema` command.
#[must_use]
pub fn format_input_error(tool_name: &str, error: &ContractValidationError) -> Value {
    let details = error
        .issues
        .iter()
        .map(|issue| {
            let path = issue.path.join(".");
            // One wording for an unknown field, alone or beside other issues.
            if issue.rule_id == "schema.unknown-field" {
                return unknown_field_message(tool_name, issue);
            }
            let message = match required_guidance(issue) {
                Some(guidance) => format!("{} ({guidance})", issue.message),
                None => issue.message.clone(),
            };
            if path.is_empty() {
                message
            } else {
                format!("{path}: {message}")
            }
        })
        .collect::<Vec<_>>();
    issues_error(details)
}

/// The repair for one unknown field: the branch that declares it, a
/// top-level row field to move, or removal with the closest known field.
fn unknown_field_message(_tool_name: &str, issue: &ValidationIssue) -> String {
    let query = query_label(&issue.path);
    let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
    if let Some(requires) = sibling_requirement(issue) {
        return format!(
            "'{field}' is not allowed in this form of {query}; it requires {requires}."
        );
    }
    let known = issue
        .schema
        .as_ref()
        .and_then(|schema| schema["knownFields"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    format!(
        "Unknown field '{field}' in {query}. Allowed fields: {}.",
        known.join(", ")
    )
}

/// The error envelope every surface returns for a call that never reached a
/// tool: the error and, when there are more, its other repair details. The
/// caller knows the tool; `error` (not `results`) marks the envelope.
pub fn tool_error(error: &str, details: Vec<String>) -> Value {
    let mut value = serde_json::json!({ "error": error });
    if !details.is_empty() {
        value["details"] = serde_json::json!(details);
    }
    value
}

/// Repair details as one error: the first names the fix, the rest follow.
fn issues_error(mut details: Vec<String>) -> Value {
    if details.is_empty() {
        return tool_error("Invalid input.", Vec::new());
    }
    let first = details.remove(0);
    tool_error(&first, details)
}

/// Core-authored guidance for a missing required field: the field's
/// contract `x-requiredMessage`, rendered after `Missing required field:
/// <name>` as MCP renders the core message.
fn required_guidance(issue: &ValidationIssue) -> Option<&str> {
    if issue.rule_id != "schema.required" {
        return None;
    }
    issue.schema.as_ref()?.get("x-requiredMessage")?.as_str()
}
