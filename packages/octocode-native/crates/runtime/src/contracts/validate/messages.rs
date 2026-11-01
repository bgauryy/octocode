use super::{ContractValidationError, ValidationIssue, query_label, sibling_requirement};
use serde_json::Value;

/// Projects transport-neutral validation issues into the stable tool-error
/// envelope. `mcp` selects the schema pointer: MCP clients read the tool's
/// `inputSchema`; only the CLI has the `schema` command.
#[must_use]
pub fn format_input_error(tool_name: &str, error: &ContractValidationError) -> Value {
    // A request field inside a row explains every other issue of that row
    // (the row lacks its own fields because it was never a row).
    let misplaced = error
        .issues
        .iter()
        .filter(|issue| misplaced_request_field(tool_name, issue).is_some())
        .filter_map(|issue| issue.path.get(..2))
        .collect::<Vec<_>>();
    let details = error
        .issues
        .iter()
        .filter(|issue| {
            misplaced_request_field(tool_name, issue).is_some()
                || !misplaced.iter().any(|row| issue.path.starts_with(row))
        })
        .map(|issue| {
            let path = issue.path.join(".");
            // One wording for an unknown field, alone or beside other issues.
            if issue.rule_id == "schema.unknown-field" {
                return unknown_field_message(tool_name, issue, unselected_row(error, issue));
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

/// Whether the row holding `issue` lacks its form selector (`operation`):
/// the form its issues come from was then scored, not chosen, so its fields
/// are not the row's allowed fields.
fn unselected_row(error: &ContractValidationError, issue: &ValidationIssue) -> bool {
    let row = &issue.path[..issue.path.len().saturating_sub(1)];
    error.issues.iter().any(|other| {
        other.rule_id == "schema.required"
            && other.path.len() == row.len() + 1
            && other.path.starts_with(row)
            && other
                .schema
                .as_ref()
                .is_some_and(|schema| schema.get("const").is_some() || schema.get("enum").is_some())
    })
}

/// The repair for one unknown field: the branch that declares it, a
/// top-level row field to move, the row field it belongs inside, or removal
/// with the closest known field. In a row without its selector, the fields
/// of every form are the allowed ones.
fn unknown_field_message(tool_name: &str, issue: &ValidationIssue, unselected: bool) -> String {
    let query = query_label(&issue.path);
    let field = issue.path.last().map(String::as_str).unwrap_or("unknown");
    if let Some(alone) = misplaced_request_field(tool_name, issue) {
        return if alone {
            format!(
                "'{field}' is a whole request, not a query field: send {{\"{field}\":\"…\"}} alone, as next.input gives it, without queries."
            )
        } else {
            format!("'{field}' is a request field: put it beside queries, not in {query}.")
        };
    }
    if let Some(requires) = sibling_requirement(issue) {
        return format!(
            "'{field}' is not allowed in this form of {query}; it requires {requires}."
        );
    }
    let row_fields = if issue.path.len() == 3 {
        authored_row_fields(tool_name)
    } else {
        Vec::new()
    };
    let known = if unselected && !row_fields.is_empty() {
        row_fields.iter().map(String::as_str).collect::<Vec<_>>()
    } else {
        issue
            .schema
            .as_ref()
            .and_then(|schema| schema["knownFields"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
    };
    let candidates = known
        .iter()
        .copied()
        .chain(row_fields.iter().map(String::as_str));
    if let Some(meant) = super::repair::closest(field, candidates) {
        return format!("Unknown field '{field}' in {query}; use '{meant}'.");
    }
    if let Some(owner) = owning_row_field(tool_name, &row_fields, field) {
        return format!(
            "'{field}' is a field of {owner}, not of {query}: send {owner}:{{\"{field}\":…}}."
        );
    }
    format!(
        "Unknown field '{field}' in {query}. Allowed fields: {}.",
        known.join(", ")
    )
}

/// Every field a caller may author in `tool_name`'s rows, across its forms:
/// a misspelt field may be meant for another form than the one it matched.
fn authored_row_fields(tool_name: &str) -> Vec<String> {
    let Some(tool) = crate::id::ToolId::from_name(tool_name) else {
        return Vec::new();
    };
    let Ok(contract) = crate::contracts::tool_contract(tool) else {
        return Vec::new();
    };
    let query = &contract["querySchema"];
    let private = crate::contracts::state_fields(tool);
    crate::contracts::schema_facts::property_names(query, &query["$defs"])
        .into_iter()
        .filter(|name| !private.contains(*name))
        .map(str::to_owned)
        .collect()
}

/// The row field (of any form) whose object value declares `field`, such as
/// astSearch `rule` for `pattern`.
fn owning_row_field<'a>(tool_name: &str, row_fields: &'a [String], field: &str) -> Option<&'a str> {
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let contract = crate::contracts::tool_contract(tool).ok()?;
    let (query, defs) = (&contract["querySchema"], &contract["querySchema"]["$defs"]);
    let facts = &crate::contracts::schema_facts::property_schemas;
    row_fields.iter().map(String::as_str).find(|name| {
        facts(query, defs, name).into_iter().any(|schema| {
            crate::contracts::schema_facts::property_names(schema, defs).contains(field)
        })
    })
}

/// For an unknown field directly in a query row (`queries.N.field`) that
/// `tool_name`'s request envelope declares: whether its envelope form holds
/// that field alone (`{cursor}`), or beside `queries` (`responseChars`).
fn misplaced_request_field(tool_name: &str, issue: &ValidationIssue) -> Option<bool> {
    let [queries, _, field] = issue.path.as_slice() else {
        return None;
    };
    if issue.rule_id != "schema.unknown-field" || queries != "queries" {
        return None;
    }
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let input = &crate::contracts::tool_contract(tool).ok()?["inputSchema"];
    input["anyOf"].as_array()?.iter().find_map(|form| {
        let properties = form["properties"].as_object()?;
        properties
            .contains_key(field.as_str())
            .then(|| properties.len() == 1)
    })
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
