//! The one lossless reading of a call whose shape or value is not the
//! contract's. An agent that sends `matchString:"x"` or `contextLines:2` means
//! exactly one thing, and refusing costs it a turn to send what the error
//! already spells out. Anything with more than one reading stays a rejection,
//! and each repair still has to pass validation, so the contract keeps deciding
//! what is valid.

use serde_json::{Map, Value};

use super::validate::{ValidationIssue, validate_batch};
use crate::id::ToolId;

/// Repair rounds a call may take: one per nesting level of its values. A union
/// reports one branch's issues at a time.
const MAX_ROUNDS: usize = 4;

/// `input` as the contract takes it, when one lossless reading exists;
/// otherwise unchanged, so validation names what is wrong with what was sent.
pub(crate) fn normalized_input(tool_name: &str, input: Value) -> Value {
    let Some(id) = ToolId::from_name(tool_name) else {
        return input;
    };
    let mut input = shaped(id, input);
    if !input.get("queries").is_some_and(Value::is_array) {
        return input;
    }
    let sent = input.clone();
    let query_schema = super::tool_contract(id)
        .map(|tool| tool["querySchema"].clone())
        .unwrap_or(Value::Null);
    let defs = query_schema["$defs"].clone();
    let mut operations = Vec::new();
    operation_values(&query_schema, &defs, &mut operations);
    for _ in 0..MAX_ROUNDS {
        let Ok((_, batch)) = validate_batch(tool_name, input.clone(), false) else {
            return sent;
        };
        let issues: Vec<_> = batch
            .rows
            .iter()
            .flatten()
            .chain(&batch.envelope)
            .cloned()
            .collect();
        if issues.is_empty() {
            return input;
        }
        if !repair_values(tool_name, &mut input, &issues, (&defs, &operations))
            && !unwrap_lone_selections(&mut input, &batch.rows)
        {
            return input;
        }
    }
    sent
}

/// The envelope a request means: a bare row or list of rows wrapped, a row
/// field beside `queries` given to each row, an envelope field in a row
/// moved beside `queries`. A field of neither leaves the shape to validation.
fn shaped(id: ToolId, input: Value) -> Value {
    let mut request = match input {
        Value::Array(rows) if !rows.is_empty() && rows.iter().all(Value::is_object) => {
            let mut request = Map::new();
            request.insert("queries".to_owned(), Value::Array(rows));
            request
        }
        Value::Object(request) => request,
        other => return other,
    };
    let declared_envelope = envelope_fields(id);
    if let Some(queries) = request.remove("queries") {
        let queries = match queries {
            Value::String(text) => match serde_json::from_str::<Value>(&text) {
                Ok(parsed @ (Value::Array(_) | Value::Object(_))) => parsed,
                _ => Value::String(text),
            },
            other => other,
        };
        let queries = match queries {
            row @ Value::Object(_) => Value::Array(vec![row]),
            other => other,
        };
        request.insert("queries".to_owned(), queries);
    } else if request.contains_key("cursor") {
        // A cursor keeps the page size it was issued with.
        request.remove("responseChars");
        return Value::Object(request);
    } else {
        let (row, envelope): (Map<_, _>, Map<_, _>) = request
            .into_iter()
            .partition(|(field, _)| super::query_declares(id, field));
        let unknown = envelope
            .keys()
            .any(|field| !declared_envelope.iter().any(|known| known == field));
        if row.is_empty() || unknown {
            let mut unchanged = row;
            unchanged.extend(envelope);
            return Value::Object(unchanged);
        }
        request = envelope;
        request.insert("queries".to_owned(), Value::Array(vec![Value::Object(row)]));
    }
    spread_row_fields(id, &mut request, &declared_envelope);
    lift_envelope_fields(id, &mut request, &declared_envelope);
    for row in rows_mut(&mut request) {
        complete_sort(row);
    }
    Value::Object(request)
}

/// A sort names what to sort by; its direction has one natural reading. Names
/// read in alphabetical order, and counts, sizes and dates read biggest first.
/// A pair the tool does not take still fails validation.
fn complete_sort(row: &mut Map<String, Value>) {
    let Some(sort) = row.get_mut("sort") else {
        return;
    };
    if sort.is_string() {
        *sort = serde_json::json!({"by": sort.take()});
    }
    let Some(sort) = sort.as_object_mut() else {
        return;
    };
    let order = match sort.get("by").and_then(Value::as_str) {
        Some("path" | "name" | "traversal") => "asc",
        Some(_) => "desc",
        None => return,
    };
    sort.entry("order")
        .or_insert_with(|| Value::String(order.to_owned()));
}

/// The request's own fields, `queries` aside.
fn envelope_fields(id: ToolId) -> Vec<String> {
    super::tool_contract(id)
        .map(|tool| {
            let input = &tool["inputSchema"];
            super::schema_facts::property_names(input, &input["$defs"])
                .into_iter()
                .filter(|field| *field != "queries")
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A row field beside `queries` applies to every row that does not set it.
fn spread_row_fields(id: ToolId, request: &mut Map<String, Value>, envelope: &[String]) {
    let shared: Vec<String> = request
        .keys()
        .filter(|field| {
            *field != "queries"
                && !envelope.iter().any(|known| known == *field)
                && super::query_declares(id, field)
        })
        .cloned()
        .collect();
    for field in shared {
        let Some(value) = request.remove(&field) else {
            continue;
        };
        for row in rows_mut(request) {
            row.entry(field.clone()).or_insert_with(|| value.clone());
        }
    }
}

/// An envelope field in a row moves beside `queries` when the rows agree on
/// it; rows that disagree have no one reading.
fn lift_envelope_fields(id: ToolId, request: &mut Map<String, Value>, envelope: &[String]) {
    for field in envelope {
        if super::query_declares(id, field) || request.contains_key(field) {
            continue;
        }
        let values: Vec<Value> = rows_mut(request)
            .filter_map(|row| row.get(field).cloned())
            .collect();
        let Some(first) = values.first().cloned() else {
            continue;
        };
        if values.iter().any(|value| *value != first) {
            continue;
        }
        for row in rows_mut(request) {
            row.remove(field);
        }
        request.insert(field.clone(), first);
    }
}

fn rows_mut(request: &mut Map<String, Value>) -> impl Iterator<Item = &mut Map<String, Value>> {
    request
        .get_mut("queries")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// A filter that takes one value, sent as a one-item `{include:[value]}`, the
/// shape its sibling query form takes (`committedAt` with and without
/// `keywords`). Only on an invalid row, and only an object value: validation of
/// the result decides.
fn unwrap_lone_selections(input: &mut Value, rows: &[Vec<ValidationIssue>]) -> bool {
    let mut changed = false;
    for (index, issues) in rows.iter().enumerate() {
        if issues.is_empty() {
            continue;
        }
        let Some(filters) = input["queries"][index]
            .get_mut("filters")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        for value in filters.values_mut() {
            let lone = value.as_object().filter(|selection| selection.len() == 1);
            let item = lone
                .and_then(|selection| selection.get("include"))
                .and_then(Value::as_array)
                .filter(|items| items.len() == 1 && items[0].is_object())
                .map(|items| items[0].clone());
            if let Some(item) = item {
                *value = item;
                changed = true;
            }
        }
    }
    changed
}

/// A filter selection is `{include, exclude}`: a bare value of it means
/// `include`. Whether the item fits is validation's call.
fn repair_selection(input: &mut Value, issue: &ValidationIssue) -> bool {
    let Some((_, parent)) = issue.path.split_last() else {
        return false;
    };
    let takes_selection = issue
        .schema
        .as_ref()
        .and_then(|schema| schema["knownFields"].as_array())
        .is_some_and(|known| {
            known.iter().any(|field| field == "include")
                && known
                    .iter()
                    .all(|field| field == "include" || field == "exclude")
        });
    let Some(at) = input.pointer_mut(&pointer(parent)) else {
        return false;
    };
    if !takes_selection || at.get("include").is_some() || at.get("exclude").is_some() {
        return false;
    }
    *at = serde_json::json!({"include": [at.take()]});
    true
}

/// Whether `path` ends at an array index below its query row.
fn is_list_item(path: &[String]) -> bool {
    path.len() > 2
        && path
            .last()
            .is_some_and(|segment| segment.parse::<usize>().is_ok())
}

fn pointer(path: &[String]) -> String {
    path.iter()
        .map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1")))
        .collect()
}

/// A row without `operation` whose fields only one operation accepts means that
/// operation. Several accepting it, or none, leaves the choice to the caller.
fn repair_operation(
    tool_name: &str,
    input: &mut Value,
    issue: &ValidationIssue,
    operations: &[String],
) -> bool {
    let [queries, index, field] = issue.path.as_slice() else {
        return false;
    };
    if queries != "queries" || field != "operation" || issue.rule_id != "schema.required" {
        return false;
    }
    let Some(row) = input.pointer(&pointer(&issue.path[..2])).cloned() else {
        return false;
    };
    let accepting: Vec<&String> = operations
        .iter()
        .filter(|operation| {
            let mut candidate = row.clone();
            candidate["operation"] = Value::String((*operation).clone());
            validate_batch(
                tool_name,
                serde_json::json!({"queries": [candidate]}),
                false,
            )
            .is_ok_and(|(_, batch)| batch.rows.iter().flatten().next().is_none())
        })
        .collect();
    let [only] = accepting.as_slice() else {
        return false;
    };
    let Ok(index) = index.parse::<usize>() else {
        return false;
    };
    input["queries"][index]["operation"] = Value::String((*only).clone());
    true
}

/// Every value of `operation` a query schema names, through its variants.
fn operation_values(schema: &Value, defs: &Value, values: &mut Vec<String>) {
    let schema = super::schema_facts::resolve_ref(schema, defs);
    if let Some(operation) = schema
        .get("properties")
        .and_then(|fields| fields.get("operation"))
    {
        let operation = super::schema_facts::resolve_ref(operation, defs);
        let named = operation
            .get("enum")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .chain(operation.get("const"));
        for value in named.filter_map(Value::as_str) {
            if !values.iter().any(|known| known == value) {
                values.push(value.to_owned());
            }
        }
    }
    for branch in ["anyOf", "oneOf", "allOf"]
        .iter()
        .filter_map(|keyword| schema.get(*keyword).and_then(Value::as_array))
        .flatten()
    {
        operation_values(branch, defs, values);
    }
}

/// Apply each issue's repair at its path; whether any applied.
fn repair_values(
    tool_name: &str,
    input: &mut Value,
    issues: &[ValidationIssue],
    (defs, operations): (&Value, &[String]),
) -> bool {
    let mut applied = false;
    for issue in issues {
        if issue.rule_id == "schema.required" {
            applied |= repair_operation(tool_name, input, issue, operations);
            continue;
        }
        if issue.rule_id == "schema.unknown-field" {
            applied |= repair_selection(input, issue);
            continue;
        }
        let (Some(schema), Some(received)) = (&issue.schema, &issue.received) else {
            continue;
        };
        if issue.rule_id != "schema.type" {
            continue;
        }
        // A number in a list may be half of something: `ranges:[2,4]` could mean
        // lines 2 and 4 or lines 2 through 4. A list item has no one reading.
        if received.is_number() && is_list_item(&issue.path) {
            continue;
        }
        let Some(repaired) = super::validate::repaired_value(schema, defs, received) else {
            continue;
        };
        if let Some(at) = input.pointer_mut(&pointer(&issue.path))
            && *at == *received
        {
            *at = repaired;
            applied = true;
        }
    }
    applied
}

#[cfg(test)]
mod tests;
