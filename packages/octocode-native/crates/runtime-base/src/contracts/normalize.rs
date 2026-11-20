//! The one lossless reading of a call whose shape or value is not the
//! contract's. An agent that sends `matchString:"x"` or `contextLines:2` means
//! exactly one thing, and refusing costs it a turn to send what the error
//! already spells out. Anything with more than one reading stays a rejection,
//! and each repair still has to pass validation, so the contract keeps deciding
//! what is valid.

use serde_json::{Map, Value};

use super::validate::{ValidationIssue, validate_batch};
use crate::id::ToolId;

mod beside_cursor;
mod duration_sign;
mod github_location;
mod guesses;
mod notes;
pub use beside_cursor::{BesideCursor, read_beside_cursor};
pub use guesses::meant_field;
use notes::{Note, Repair};

/// `input` as the contract takes it, when one lossless reading exists;
/// otherwise unchanged, so validation names what is wrong with what was sent.
/// The notes say each repair: which sent fields were read as declared ones
/// ([`guesses`]) and which values in the shape their field takes, so the
/// caller learns the field and form to send. A field is said once per row,
/// with the value it runs with, which is the form to send on a row that
/// runs; a row that still fails says only what was read ([`notes`]).
pub fn normalized_input(tool_name: &str, input: Value) -> (Value, Vec<String>) {
    let (input, notes) = normalized(tool_name, input);
    let failing = failing_rows(tool_name, &input, &notes);
    let said = notes::per_call(notes, &input, &failing);
    (input, said)
}

/// The rows of `input` that still fail validation as they run, once a
/// repair was made: on such a row the form a repair ran is not one the
/// contract takes, so its note names no form to send and the row's error
/// leads.
fn failing_rows(tool_name: &str, input: &Value, notes: &[Note]) -> Vec<usize> {
    if notes.is_empty() {
        return Vec::new();
    }
    validate_batch(tool_name, input.clone(), false)
        .map(|(_, batch)| {
            batch
                .rows
                .iter()
                .enumerate()
                .filter(|(_, issues)| !issues.is_empty())
                .map(|(row, _)| row)
                .collect()
        })
        .unwrap_or_default()
}

/// [`normalized_input`] with its repairs unsaid.
fn normalized(tool_name: &str, input: Value) -> (Value, Vec<Note>) {
    let Some(id) = ToolId::from_name(tool_name) else {
        return (input, Vec::new());
    };
    let (input, resumed) = cursor_alone(input);
    if resumed.is_some() {
        return (
            input,
            resumed
                .into_iter()
                .map(|note| Note::Said(None, note))
                .collect(),
        );
    }
    let mut notes = Vec::new();
    let mut input = shaped(id, input, &mut notes);
    if !input.get("queries").is_some_and(Value::is_array) {
        return (input, notes);
    }
    let sent = input.clone();
    let mut reshaped_paths = Vec::new();
    if let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for (index, row) in rows.iter_mut().enumerate() {
            if let Some(row) = row.as_object_mut()
                && complete_sort(row)
            {
                reshaped_paths.push(vec![
                    "queries".to_owned(),
                    index.to_string(),
                    "sort".to_owned(),
                ]);
            }
        }
    }
    notes.extend(guesses::read(id, &mut input));
    let (input, repairs) = repaired(tool_name, id, input);
    reshaped_paths.extend(repairs.reshaped);
    notes.extend(repairs.notes);
    notes.extend(reshaped(&sent, &reshaped_paths));
    (input, notes)
}

/// One repair per value [`repair_values`] read in the shape its field
/// takes, of the outermost repaired value as sent: a JSON text holding JSON
/// text is one value. A value the caller did not send there was written by
/// a guess, whose repair says it.
fn reshaped(sent: &Value, paths: &[Vec<String>]) -> Vec<Note> {
    let outermost = |path: &&Vec<String>| {
        !paths
            .iter()
            .any(|outer| outer.len() < path.len() && path.starts_with(outer))
    };
    paths
        .iter()
        .filter(outermost)
        .filter_map(|path| {
            let (row, at) = match path.as_slice() {
                [queries, index, at @ ..] if queries == "queries" && !at.is_empty() => {
                    (Some(index.parse::<usize>().ok()?), at)
                }
                at => (None, at),
            };
            let sent = sent.pointer(&crate::json_pointer::of(path))?;
            Some(Note::Field(Repair::value(row, at, sent, at.to_vec())))
        })
        .collect()
}

/// Longest text a note quotes whole; a longer one keeps its two ends.
const NOTE_TEXT_CHARS: usize = 40;
/// Items of a list a note shows before one `…` stands for the rest.
const NOTE_ITEMS: usize = 3;

/// `value` as a note quotes it: JSON in its own shape, a long text kept by
/// its two ends (`"first…last"`) and a long list by its first items, so a
/// note on any value still shows the form to send.
pub(super) fn sketch(value: &Value) -> String {
    match value {
        Value::String(text) if text.chars().count() > NOTE_TEXT_CHARS => {
            let chars: Vec<char> = text.chars().collect();
            let half = NOTE_TEXT_CHARS / 2;
            let ends: String = chars[..half]
                .iter()
                .chain(&['…'])
                .chain(&chars[chars.len() - half..])
                .collect();
            Value::String(ends).to_string()
        }
        Value::Array(items) => {
            let mut shown: Vec<String> = items.iter().take(NOTE_ITEMS).map(sketch).collect();
            if items.len() > NOTE_ITEMS {
                shown.push("…".to_owned());
            }
            format!("[{}]", shown.join(","))
        }
        Value::Object(fields) => {
            let shown: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}:{}", Value::String(name.clone()), sketch(value)))
                .collect();
            format!("{{{}}}", shown.join(","))
        }
        other => other.to_string(),
    }
}

/// What [`repaired`] changed beyond the shape validation reads: the repairs
/// the caller learns a field or value from, and the path of each value read
/// in its field's shape.
#[derive(Default)]
struct Repairs {
    notes: Vec<Note>,
    reshaped: Vec<Vec<String>>,
}

/// The page size a request sets; a cursor carries the one its walk was cut at.
const PAGE_SIZE: &str = "responseChars";

/// A cursor call that also sends a page size, as the cursor alone, with the
/// note that says so. The cursor names a page of a walk cut at the size it
/// was issued with (another size would cut other pages, so the same cursor
/// would skip or repeat rows): the sent size has one reading, no effect.
/// A query field beside a cursor is read against the query the cursor
/// restores, once it is opened ([`read_beside_cursor`]); validation refuses
/// any other.
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
fn repaired(tool_name: &str, id: ToolId, mut input: Value) -> (Value, Repairs) {
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
    let mut repairs = Repairs::default();
    loop {
        let Ok((_, batch)) = validate_batch(tool_name, input.clone(), false) else {
            return (sent, Repairs::default());
        };
        let issues: Vec<_> = batch
            .rows
            .iter()
            .flatten()
            .chain(&batch.envelope)
            .cloned()
            .collect();
        if issues.is_empty() {
            return (input, repairs);
        }
        if seen.contains(&input) {
            return (sent, Repairs::default());
        }
        seen.push(input.clone());
        if !repair_values(tool_name, &mut input, &issues, round, &mut repairs) {
            return (input, repairs);
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
/// field beside `queries` given to each row, a request field declared
/// beside `queries` moved there from the rows. A field of neither leaves the
/// shape to validation.
fn shaped(id: ToolId, input: Value, notes: &mut Vec<Note>) -> Value {
    let mut request = match input {
        Value::Array(rows) if !rows.is_empty() && rows.iter().all(Value::is_object) => {
            let mut request = Map::new();
            request.insert("queries".to_owned(), Value::Array(rows));
            notes.push(Note::Said(
                None,
                "read the bare list as query rows; send {\"queries\":[…]} next time.".to_owned(),
            ));
            request
        }
        Value::Object(request) => request,
        other => return other,
    };
    let declared_envelope = envelope_fields(id);
    if let Some(queries) = request.remove("queries") {
        let mut reshaped = queries.is_object();
        let queries = match queries {
            Value::String(text) => match serde_json::from_str::<Value>(&text) {
                Ok(parsed @ (Value::Array(_) | Value::Object(_))) => {
                    reshaped = true;
                    parsed
                }
                _ => Value::String(text),
            },
            other => other,
        };
        let queries = match queries {
            row @ Value::Object(_) => Value::Array(vec![row]),
            other => other,
        };
        request.insert("queries".to_owned(), queries);
        if reshaped {
            notes.push(Note::Said(None, "read `queries` as an array of query rows; send `queries` as a JSON array of objects, not an object or a string holding JSON, next time.".to_owned()));
        }
    } else if request.contains_key("cursor") {
        // A cursor call is the cursor alone ([`cursor_alone`] has dropped a
        // page size sent beside it); a query field beside it is read against
        // the cursor's saved query ([`read_beside_cursor`]).
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
        notes.push(Note::Said(None, "read the bare query fields as one row; send {\"queries\":[{…}]} with request fields beside `queries` next time.".to_owned()));
    }
    spread_row_fields(id, &mut request, &declared_envelope, notes);
    lift_envelope_fields(id, &mut request, &beside_queries(id), notes);
    Value::Object(request)
}

/// A sort names what to sort by; its direction has one natural reading. Names
/// read in alphabetical order, and counts, sizes and dates read biggest first.
/// A pair the tool does not take still fails validation.
fn complete_sort(row: &mut Map<String, Value>) -> bool {
    let Some(sort) = row.get_mut("sort") else {
        return false;
    };
    let reshaped = sort.is_string();
    if reshaped {
        *sort = serde_json::json!({"by": sort.take()});
    }
    let Some(sort) = sort.as_object_mut() else {
        return reshaped;
    };
    let order = match sort.get("by").and_then(Value::as_str) {
        Some("path" | "name" | "traversal") => "asc",
        Some(_) => "desc",
        None => return reshaped,
    };
    let missing_order = !sort.contains_key("order");
    sort.entry("order")
        .or_insert_with(|| Value::String(order.to_owned()));
    reshaped || missing_order
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

/// The request fields the form holding `queries` declares beside it
/// (`responseChars`). A field another form holds (`cursor`, sent alone)
/// never sits beside rows, so a row sending it is no row to lift it from.
fn beside_queries(id: ToolId) -> Vec<String> {
    let Ok(tool) = super::tool_contract(id) else {
        return Vec::new();
    };
    let input = &tool["inputSchema"];
    let forms: Vec<&Value> = match input["anyOf"].as_array() {
        Some(forms) => forms.iter().collect(),
        None => vec![input],
    };
    forms
        .into_iter()
        .map(|form| super::schema_facts::resolve_ref(form, &input["$defs"]))
        .filter_map(|form| form["properties"].as_object())
        .filter(|properties| properties.contains_key("queries"))
        .flat_map(|properties| properties.keys())
        .filter(|field| *field != "queries")
        .cloned()
        .collect()
}

/// A row field beside `queries` applies to every row that does not set it.
fn spread_row_fields(
    id: ToolId,
    request: &mut Map<String, Value>,
    envelope: &[String],
    notes: &mut Vec<Note>,
) {
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
        notes.push(Note::Said(None, format!("read `{field}` beside `queries` as a shared row field, keeping each row's explicit value; send `{field}` inside the applicable `queries` rows next time.")));
    }
}

/// A request field declared beside `queries`, sent in a row, moves beside
/// `queries` when the rows agree on it; rows that disagree have no one
/// reading.
fn lift_envelope_fields(
    id: ToolId,
    request: &mut Map<String, Value>,
    envelope: &[String],
    notes: &mut Vec<Note>,
) {
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
        notes.push(Note::Said(None, format!("moved `{field}` from query rows beside `queries`; send `{field}` beside `queries` next time.")));
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
/// the repair.
fn repair_literal(input: &mut Value, issue: &ValidationIssue) -> Option<Repair> {
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
    *slot = meant;
    Some(Repair::value(Some(index), at, received, at.to_vec()))
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
/// `view:"matchOnly"`): it says what omitting it says, so it is left out,
/// and the note says so.
fn drop_default(input: &mut Value, issue: &ValidationIssue) -> Option<(Option<usize>, String)> {
    let default = issue.schema.as_ref()?.get("siblingDefault")?;
    let (field, parent) = issue.path.split_last()?;
    let row = input
        .pointer_mut(&crate::json_pointer::of(parent))
        .and_then(Value::as_object_mut)?;
    if row.get(field) != Some(default) {
        return None;
    }
    row.remove(field);
    let (index, label) = match issue.path.as_slice() {
        [queries, index, at @ ..] if queries == "queries" && !at.is_empty() => {
            (index.parse::<usize>().ok(), at.join("."))
        }
        at => (None, at.join(".")),
    };
    Some((
        index,
        format!(
            "left out `{label}: {}`, the default of a field this form of the row takes no value for; leave `{label}` out next time.",
            sketch(default)
        ),
    ))
}

/// Whether a call of `tool_name` bills a classification provider for every
/// row it runs: the tool that does not run without the classification
/// credential (clasify). A repair is a guessed reading; a free call may run
/// one and say so, but a billed call would spend on a query nobody sent.
fn bills(tool_name: &str) -> bool {
    ToolId::from_name(tool_name) == Some(ToolId::Clasify)
}

/// A query row another tool's contract checks (a clasify `source.query`,
/// core `x-toolQuery`), repaired as a direct call of that tool is: the
/// repaired row in its place, and each of that call's notes said of the
/// field that holds it (`source.query: read …`). `None` when the issue is
/// no such rule's, the direct call's repair changes nothing, or the holding
/// call bills ([`bills`]): a nested query there is validated as sent, and
/// its error names the field to send, before anything is read or billed.
fn repair_tool_query(
    tool_name: &str,
    input: &mut Value,
    issue: &ValidationIssue,
) -> Option<Vec<Note>> {
    if bills(tool_name) {
        return None;
    }
    let (tool_field, query_field) = super::validate::tool_query_fields(&issue.rule_id)?;
    let at = (0..issue.path.len()).find(|at| {
        issue.path[*at] == query_field
            && input
                .pointer(&crate::json_pointer::of(&issue.path[..*at]))
                .is_some_and(|holder| holder[tool_field].is_string())
    })?;
    let row = match issue.path.as_slice() {
        [queries, index, ..] if queries == "queries" => index.parse::<usize>().ok(),
        _ => None,
    };
    let start = usize::from(row.is_some()) * 2;
    let label = issue.path.get(start..=at)?.join(".");
    let holder = input.pointer_mut(&crate::json_pointer::of(&issue.path[..at]))?;
    let tool = holder[tool_field].as_str()?.to_owned();
    let query = holder.get(query_field)?;
    let (mut ran, notes) = normalized(&tool, serde_json::json!({ "queries": [query.clone()] }));
    let failing = failing_rows(&tool, &ran, &notes);
    let said = notes::said(notes, &ran, &failing);
    let repaired = ran["queries"].get_mut(0)?.take();
    if &repaired == query {
        return None;
    }
    holder[query_field] = repaired;
    Some(
        said.into_iter()
            .map(|(_, note)| Note::Said(row, format!("{label}: {note}")))
            .collect(),
    )
}

/// Apply each issue's repair at its path, adding its note, or its path when
/// the value was read in its field's shape ([`reshaped`] says those
/// once the call is final); whether any applied.
fn repair_values(
    tool_name: &str,
    input: &mut Value,
    issues: &[ValidationIssue],
    Round { defs, operations }: Round<'_>,
    repairs: &mut Repairs,
) -> bool {
    let mut applied = false;
    for issue in issues {
        if let Some(notes) = repair_tool_query(tool_name, input, issue) {
            applied = true;
            repairs.notes.extend(notes);
            continue;
        }
        let note = match issue.rule_id.as_str() {
            "schema.required" => repair_operation(tool_name, input, issue, operations)
                .map(|(index, note)| Note::Said(Some(index), note)),
            "schema.pattern" => github_location::repair(input, issue)
                .map(|(index, note)| Note::Said(Some(index), note))
                .or_else(|| {
                    duration_sign::repair(input, issue).map(|(index, note)| Note::Said(index, note))
                }),
            "schema.enum" => repair_literal(input, issue).map(Note::Field).or_else(|| {
                repair_returned(tool_name, input, issue)
                    .map(|(index, note)| Note::Said(Some(index), note))
            }),
            _ => None,
        };
        if let Some(note) = note {
            applied = true;
            repairs.notes.push(note);
            continue;
        }
        if issue.rule_id == "schema.required" {
            continue;
        }
        if issue.rule_id == "schema.unknown-field" {
            if let Some((index, note)) = drop_default(input, issue) {
                applied = true;
                repairs.notes.push(Note::Said(index, note));
            }
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
            repairs.reshaped.push(issue.path.clone());
        }
    }
    applied
}

#[cfg(test)]
mod tests;
