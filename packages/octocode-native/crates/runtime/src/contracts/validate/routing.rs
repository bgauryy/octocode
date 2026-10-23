use super::{
    ValidationIssue, cover_field, envelope_field, name_field, query_label, sibling_requirement,
    suggest_field,
};
use crate::id::ToolId;
use serde_json::Value;

/// A rejected row's label, every field it sets, and its unknown fields with
/// whether each has an in-tool fix.
type RowFields<'a> = (String, Vec<&'a str>, Vec<(&'a str, bool)>);

/// Guidance that names the fix beyond the field-level issues: the tool that
/// owns a row's unknown fields or operation, and how to split an oversized
/// batch.
pub(super) fn routing_details(tool_name: &str, issues: &[ValidationIssue]) -> Vec<String> {
    let mut details = Vec::new();
    // Per row: every field it sets, and its unknown fields with whether each
    // has an in-tool fix.
    let mut rows: Vec<RowFields> = Vec::new();
    for issue in issues {
        match (issue.rule_id.as_str(), issue.path.as_slice()) {
            ("schema.unknown-field", [queries, _, field]) if queries == "queries" => {
                let label = query_label(&issue.path);
                let known = issue
                    .schema
                    .as_ref()
                    .and_then(|s| s["knownFields"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                // A field another form of this tool declares is fixed in
                // place (`it applies only with …`), never rerouted.
                let in_tool = suggest_field(field, &known).is_some()
                    || name_field(issue).is_some()
                    || sibling_requirement(issue).is_some()
                    || cover_field(issue).is_some()
                    || envelope_field(issue);
                let entry = (field.as_str(), in_tool);
                match rows.iter_mut().find(|(row, ..)| *row == label) {
                    Some((_, _, fields)) => fields.push(entry),
                    None => {
                        let row_fields = issue
                            .schema
                            .as_ref()
                            .and_then(|s| s["rowFields"].as_array())
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        rows.push((label, row_fields, vec![entry]));
                    }
                }
            }
            ("schema.enum", [.., last]) if last == "operation" => {
                if let Some(operation) = issue.received.as_ref().and_then(Value::as_str) {
                    let owners = owning_tools(tool_name, |tool| {
                        operations(tool).iter().any(|name| name == operation)
                    });
                    if let Some(owners) = owners {
                        details.push(format!(
                            "operation \"{operation}\" is a {owners} operation: send that row to {owners}."
                        ));
                    }
                }
            }
            ("schema.range", [queries, _, field])
                if queries == "queries" && field == "length" && windowed(tool_name) =>
            {
                if let Some(message) = window_cap_message(issue) {
                    details.push(message);
                }
            }
            ("schema.size", [queries]) if queries == "queries" => {
                if let Some(message) = split_batch_message(&issue.message) {
                    details.push(message);
                }
            }
            _ => {}
        }
    }
    for (label, mut row_fields, mut entries) in rows {
        entries.sort_unstable();
        entries.dedup();
        // One misspelled field the tool itself can name needs no rerouting.
        if let [(_, true)] = entries.as_slice() {
            continue;
        }
        let fields = entries.iter().map(|(field, _)| *field).collect::<Vec<_>>();
        // The whole row goes, so the other tool must run all of it.
        row_fields.extend(&fields);
        let owners = owning_tools(tool_name, |tool| runs_row(tool, &row_fields));
        if let Some(owners) = owners {
            let (subject, noun) = if fields.len() == 1 {
                (format!("{} is a", fields[0]), "field")
            } else {
                (format!("{} are", fields.join(", ")), "fields")
            };
            details.push(format!(
                "{subject} {owners} {noun}: send {label} to {owners}."
            ));
        }
    }
    details.dedup();
    details
}

/// A `length` above the window cap → the same read at the cap, and where the
/// rest is: every result's `next` continuation carries the following offset.
/// A tool whose rows read a window: `length` pages from an `offset`.
fn windowed(tool_name: &str) -> bool {
    crate::id::ToolId::from_name(tool_name)
        .is_some_and(|tool| super::super::query_declares(tool, "offset"))
}

fn window_cap_message(issue: &ValidationIssue) -> Option<String> {
    let cap = issue.schema.as_ref()?.get("maximum")?.as_u64()?;
    (issue.received.as_ref()?.as_u64()? > cap).then(|| {
        format!(
            "Send the same read with \"length\": {cap}: the result's next carries the offset of the rest \
             (copy it into the following call) until a page has no next."
        )
    })
}

/// `Value length 7 exceeds the maximum of 5` → how many calls to send.
fn split_batch_message(message: &str) -> Option<String> {
    let numbers = message
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse::<usize>().ok())
        .collect::<Vec<_>>();
    let [length, maximum] = numbers.as_slice() else {
        return None;
    };
    (message.contains("exceeds") && *maximum > 0).then(|| {
        format!(
            "Send at most {maximum} rows per call: split the batch into {} calls.",
            length.div_ceil(*maximum)
        )
    })
}

/// The one other non-beta tool of the same family (or `a or b` for two)
/// whose query schema satisfies `owns`; `None` when no tool or too many do.
fn owning_tools(tool_name: &str, owns: impl Fn(&Value) -> bool) -> Option<String> {
    // Family and beta are generated from the same contract fields, so only
    // same-family candidates are parsed.
    let family = ToolId::from_name(tool_name)?.family();
    let owners = ToolId::ALL
        .into_iter()
        .filter(|id| id.as_str() != tool_name && !id.is_beta() && id.family() == family)
        .filter_map(|id| super::super::tool_contract(id).ok())
        .filter(|tool| owns(tool))
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    match owners.as_slice() {
        [one] => Some((*one).to_owned()),
        [a, b] => Some(format!("{a} or {b}")),
        _ => None,
    }
}

/// Whether one form of a tool's query schema takes a row of `fields`: it
/// declares (and does not forbid) every one, and requires none the row
/// lacks unless it has a default.
fn runs_row(tool: &Value, fields: &[&str]) -> bool {
    let mut runs = false;
    walk_query_schema(tool, &mut |schema| {
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return;
        };
        let declares = |field: &str| {
            properties
                .get(field)
                .is_some_and(|property| property.get("not").is_none())
        };
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .all(|field| {
                fields.contains(&field)
                    || properties.get(field).is_some_and(|property| {
                        super::super::resolve_ref(property, &tool["querySchema"]["$defs"])
                            .get("default")
                            .is_some()
                    })
            });
        runs |= required && fields.iter().all(|field| declares(field));
    });
    runs
}

/// Every `operation` value any branch of a tool's query schema allows.
fn operations(tool: &Value) -> Vec<String> {
    let mut values = Vec::new();
    walk_query_schema(tool, &mut |schema| {
        if let Some(operation) = schema.pointer("/properties/operation") {
            let pinned = operation.get("const").into_iter();
            let listed = operation
                .get("enum")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            values.extend(
                pinned
                    .chain(listed)
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
    });
    values
}

fn walk_query_schema(tool: &Value, visit: &mut dyn FnMut(&Value)) {
    fn walk(root: &Value, schema: &Value, depth: usize, visit: &mut dyn FnMut(&Value)) {
        if depth > 16 {
            return;
        }
        let schema = match schema.get("$ref").and_then(Value::as_str) {
            Some(reference) => match reference.strip_prefix('#') {
                Some(pointer) => root.pointer(pointer).unwrap_or(&Value::Null),
                None => return,
            },
            None => schema,
        };
        visit(schema);
        for key in ["anyOf", "oneOf", "allOf"] {
            for branch in schema
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                walk(root, branch, depth + 1, visit);
            }
        }
    }
    let root = &tool["querySchema"];
    walk(root, root, 0, visit);
}
