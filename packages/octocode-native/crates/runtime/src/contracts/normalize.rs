//! The one lossless reading of a call whose shape or value is not the
//! contract's. An agent that sends `matchString:"x"` or `contextLines:2` means
//! exactly one thing, and refusing costs it a turn to send what the error
//! already spells out. Anything with more than one reading stays a rejection,
//! and each repair still has to pass validation, so the contract keeps deciding
//! what is valid.

use serde_json::{Map, Value};

use super::validate::{ValidationIssue, validate_batch};
use crate::id::ToolId;

mod guesses;
pub(crate) use guesses::meant_field;

/// `input` as the contract takes it, when one lossless reading exists;
/// otherwise unchanged, so validation names what is wrong with what was sent.
/// The notes say which sent fields were read as declared ones
/// ([`guesses`]): the call states them, so the caller learns the field.
pub(crate) fn normalized_input(tool_name: &str, input: Value) -> (Value, Vec<String>) {
    let Some(id) = ToolId::from_name(tool_name) else {
        return (input, Vec::new());
    };
    let (input, resumed) = cursor_alone(input);
    if resumed.is_some() {
        return (input, resumed.into_iter().collect());
    }
    let mut input = shaped(id, input);
    if !input.get("queries").is_some_and(Value::is_array) {
        return (input, Vec::new());
    }
    let mut notes = guesses::read(id, &mut input);
    let (input, repairs) = repaired(tool_name, id, input);
    notes.extend(guesses::per_call(repairs));
    (input, notes)
}

/// The page size a request sets; a cursor carries the one its walk was cut at.
const PAGE_SIZE: &str = "responseChars";

/// A cursor call that also sends a page size, as the cursor alone, with the
/// note that says so. The cursor names a page of a walk cut at the size it
/// was issued with (another size would cut other pages, so the same cursor
/// would skip or repeat rows): the sent size has one reading, no effect.
/// Any other field beside a cursor is left for validation to refuse.
fn cursor_alone(input: Value) -> (Value, Option<String>) {
    let mut request = match input {
        Value::Object(request) => request,
        other => return (other, None),
    };
    let only_size_beside =
        request.contains_key("cursor") && request.len() == 2 && request.contains_key(PAGE_SIZE);
    if !only_size_beside {
        return (Value::Object(request), None);
    }
    request.remove(PAGE_SIZE);
    let note = format!(
        "`{PAGE_SIZE}` beside `cursor` was not applied: a cursor keeps the page size it was issued with; send {{\"cursor\":\"…\"}} alone, and `{PAGE_SIZE}` with queries in a new request."
    );
    (Value::Object(request), Some(note))
}

/// [`normalized_input`]'s value repairs of a shaped request, and the row
/// notes of those the caller learns a field or value from. Each round
/// repairs what validation reports, one nesting level of a value (a union
/// reports one branch's issues at a time), until the call validates or no
/// repair applies. A round that brings back an input already seen is a
/// cycle with no one reading: the call stays as sent.
fn repaired(tool_name: &str, id: ToolId, mut input: Value) -> (Value, Vec<(usize, String)>) {
    let sent = input.clone();
    let query_schema = super::tool_contract(id)
        .map(|tool| tool["querySchema"].clone())
        .unwrap_or(Value::Null);
    let defs = query_schema["$defs"].clone();
    let mut operations = Vec::new();
    operation_values(&query_schema, &defs, &mut operations);
    let round = Round {
        defs: &defs,
        operations: &operations,
    };
    let mut seen = Vec::new();
    let mut notes = Vec::new();
    loop {
        let Ok((_, batch)) = validate_batch(tool_name, input.clone(), false) else {
            return (sent, Vec::new());
        };
        let issues: Vec<_> = batch
            .rows
            .iter()
            .flatten()
            .chain(&batch.envelope)
            .cloned()
            .collect();
        if issues.is_empty() {
            return (input, notes);
        }
        if seen.contains(&input) {
            return (sent, Vec::new());
        }
        seen.push(input.clone());
        if !repair_values(tool_name, &mut input, &issues, round, &mut notes) {
            return (input, notes);
        }
    }
}

/// What every round of [`repaired`] reads: the query schema's definitions
/// and every `operation` it names.
#[derive(Clone, Copy)]
struct Round<'a> {
    defs: &'a Value,
    operations: &'a [String],
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
        // A cursor call is the cursor alone ([`cursor_alone`] has dropped a
        // page size sent beside it); validation refuses any other field.
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

/// Whether `path` ends at an array index below its query row.
fn is_list_item(path: &[String]) -> bool {
    path.len() > 2
        && path
            .last()
            .is_some_and(|segment| segment.parse::<usize>().is_ok())
}

/// A row without `operation` whose fields only one operation accepts means that
/// operation, and the note says so. Several accepting it, or none, leaves the
/// choice to the caller.
fn repair_operation(
    tool_name: &str,
    input: &mut Value,
    issue: &ValidationIssue,
    operations: &[String],
) -> Option<(usize, String)> {
    let [queries, index, field] = issue.path.as_slice() else {
        return None;
    };
    if queries != "queries" || field != "operation" || issue.rule_id != "schema.required" {
        return None;
    }
    let row = input
        .pointer(&crate::json_pointer::of(&issue.path[..2]))
        .cloned()?;
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
        return None;
    };
    let index = index.parse::<usize>().ok()?;
    input["queries"][index]["operation"] = Value::String((*only).clone());
    Some((
        index,
        format!(
            "read the row as `operation: \"{only}\"`, the one operation its fields fit; send `operation: \"{only}\"` next time."
        ),
    ))
}

/// A text sent for a field whose literals (`enum`) hold one value it spells
/// in another case or separator style (`pypi` for `PyPI`): that value, and
/// the note that names it.
fn repair_literal(input: &mut Value, issue: &ValidationIssue) -> Option<(usize, String)> {
    if issue.rule_id != "schema.enum" {
        return None;
    }
    let (schema, received) = (issue.schema.as_ref()?, issue.received.as_ref()?);
    let meant = super::validate::literal_reading(schema, received)?;
    let [queries, index, at @ ..] = issue.path.as_slice() else {
        return None;
    };
    let index = index
        .parse::<usize>()
        .ok()
        .filter(|_| queries == "queries")?;
    let slot = input.pointer_mut(&crate::json_pointer::of(&issue.path))?;
    if *slot != *received {
        return None;
    }
    *slot = meant.clone();
    let label = at.join(".");
    Some((
        index,
        format!(
            "read `{label}: {received}` as `{label}: {meant}`; send `{label}: {meant}` next time."
        ),
    ))
}

/// A list item the row's operation refuses that names what every read of
/// that operation returns ([`guesses::RETURNED`]): left out, every spelling
/// of it, and the list too when it held nothing else. The note names the
/// field that carries it.
fn repair_returned(
    tool_name: &str,
    input: &mut Value,
    issue: &ValidationIssue,
) -> Option<(usize, String)> {
    let id = ToolId::from_name(tool_name)?;
    let [queries, index, field, item] = issue.path.as_slice() else {
        return None;
    };
    let sent = issue.received.as_ref()?.as_str()?;
    if queries != "queries" || item.parse::<usize>().is_err() {
        return None;
    }
    let index = index.parse::<usize>().ok()?;
    let row = input["queries"].get_mut(index)?.as_object_mut()?;
    let operation = row.get("operation").and_then(Value::as_str)?;
    let returned = guesses::RETURNED.iter().find(|returned| {
        returned.tools.contains(&id)
            && returned.operation == operation
            && returned.field == field
            && super::validate::same_name(sent, returned.sent)
    })?;
    let note = format!(
        "left `{sent}` out of `{field}`: every {operation} read returns its whole `{}`; leave `{}` out next time.",
        returned.returned_as, returned.sent
    );
    let items = row.get_mut(field.as_str())?.as_array_mut()?;
    items.retain(|item| {
        !item
            .as_str()
            .is_some_and(|item| super::validate::same_name(item, returned.sent))
    });
    if items.is_empty() {
        row.remove(field.as_str());
    }
    Some((index, note))
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

/// A field another form of the row declares, sent at the default every such
/// form gives it (`contextLines:{before:0,after:0}` beside
/// `view:"matchOnly"`): it says what omitting it says, so it is left out.
fn drop_default(input: &mut Value, issue: &ValidationIssue) -> bool {
    let Some(default) = issue
        .schema
        .as_ref()
        .and_then(|schema| schema.get("siblingDefault"))
    else {
        return false;
    };
    let Some((field, parent)) = issue.path.split_last() else {
        return false;
    };
    let Some(row) = input
        .pointer_mut(&crate::json_pointer::of(parent))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    if row.get(field) != Some(default) {
        return false;
    }
    row.remove(field);
    true
}

/// Apply each issue's repair at its path, adding the notes of those the
/// caller learns from; whether any applied.
fn repair_values(
    tool_name: &str,
    input: &mut Value,
    issues: &[ValidationIssue],
    Round { defs, operations }: Round<'_>,
    notes: &mut Vec<(usize, String)>,
) -> bool {
    let mut applied = false;
    for issue in issues {
        let note = match issue.rule_id.as_str() {
            "schema.required" => repair_operation(tool_name, input, issue, operations),
            "schema.enum" => {
                repair_literal(input, issue).or_else(|| repair_returned(tool_name, input, issue))
            }
            _ => None,
        };
        if let Some(note) = note {
            applied = true;
            notes.push(note);
            continue;
        }
        if issue.rule_id == "schema.required" {
            continue;
        }
        if issue.rule_id == "schema.unknown-field" {
            applied |= drop_default(input, issue);
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
        if let Some(at) = input.pointer_mut(&crate::json_pointer::of(&issue.path))
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
