use super::{
    ContractValidationError, ValidationIssue, cover_field, envelope_field, field_guess,
    query_label, routing_details, sibling_message, sibling_requirement,
};
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
        .chain(routing_details(tool_name, &error.issues))
        .collect::<Vec<_>>();
    issues_error(details)
}

/// The repair for one unknown field: the branch that declares it, a
/// top-level row field to move, or removal with the closest known field.
fn unknown_field_message(tool_name: &str, issue: &ValidationIssue) -> String {
    let query = query_label(&issue.path);
    let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
    // Known fields from the embedded schema suggest the closest match (edit
    // distance <= 3) as a "did you mean ...?" hint.
    let known: Vec<&str> = issue
        .schema
        .as_ref()
        .and_then(|s| s["knownFields"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect();
    let msg = match (
        sibling_requirement(issue),
        cover_field(issue),
        field_guess(issue, field, &known),
    ) {
        (Some(requires), ..) => sibling_message(issue, field, &query, &requires),
        _ if is_root_brief(tool_name, &issue.path) => root_brief_message(field),
        _ if envelope_field(issue) => {
            format!("Move '{field}' from {query} beside queries: it applies to the whole call.")
        }
        (None, Some(cover), _) => {
            format!("Remove unknown field '{field}' from {query}: {cover} expresses it.")
        }
        (None, None, Some(s)) => {
            format!("Remove unknown field '{field}' from {query} (did you mean '{s}'?)")
        }
        (None, None, None) => format!("Remove unknown field(s) from {query}: {field}"),
    };
    if issue.path.iter().any(|segment| segment == "rule") {
        format!(
            "{}. Example rule: {}",
            msg.trim_end_matches('.'),
            octocode_engine::structural::EXAMPLE_RULE
        )
    } else {
        msg
    }
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

/// A row field (`mainGoal`, `reasoning`, …) sent beside `queries` instead of
/// inside each row: the envelope does not declare it, the rows do.
fn is_root_brief(tool_name: &str, path: &[String]) -> bool {
    matches!(path, [field] if crate::id::ToolId::from_name(tool_name)
        .is_some_and(|tool| super::super::query_declares(tool, field)))
}

fn root_brief_message(field: &str) -> String {
    format!("Move '{field}' into each queries[] row: a top-level {field} is not inherited.")
}
