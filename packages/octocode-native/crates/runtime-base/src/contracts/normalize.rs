//! Strict input with schema-derived leniency. A request is validated as
//! sent; the only other readings are the ones the contract's schema alone
//! implies and that give the value exactly one typed reading:
//!
//! - the rows without their envelope: a bare row, a bare list of rows, one
//!   row for `queries`;
//! - a value where a list of such values is declared: that one-item list;
//! - a text holding JSON of the declared type: that JSON value;
//! - an enum literal in another letter case or separator style;
//! - an integral float (`3.0`) where only an integer is declared: the integer;
//! - a `[lower, upper]` pair where an object of two ordered fields is
//!   declared (`x-order`): that object;
//! - a member an object lacks, from the default its schema states;
//! - a `matchString` item of a regex row that parses only with its unpaired
//!   brackets read as text (`def route(`): those brackets escaped;
//! - a page size beside a cursor, or a cursor as the one row of `queries`:
//!   the cursor alone.
//!
//! A call says every reading in one warning that names each field and how
//! it was read, on a row that then runs; a row that still fails is refused
//! as read, for what is still wrong with it. None is applied to a call that bills. No field is
//! read under another name, moved into another object or inferred: an
//! undeclared field is refused, and the refusal names the declared one.

use serde_json::{Map, Value};

use super::validate::{ValidationIssue, validate_batch};
use crate::id::ToolId;

mod beside_cursor;
mod integers;
mod notes;
pub use beside_cursor::{BesideCursor, read_beside_cursor};

/// One lenient reading, said of the row it was made on (`None`: the
/// request): the field and how it was read ([`notes::per_call`] joins them).
type Note = (Option<usize>, String);

/// `input` as the contract takes it, with one warning naming the lenient
/// readings made (the module's list); otherwise unchanged, so validation
/// names what is wrong with what was sent.
pub fn normalized_input(tool_name: &str, input: Value) -> (Value, Vec<String>) {
    let Some(id) = ToolId::from_name(tool_name) else {
        return (input, Vec::new());
    };
    let (input, resumed) = cursor_alone(input);
    if let Some(note) = resumed {
        return (input, vec![note]);
    }
    let mut notes = Vec::new();
    let request = shaped(id, input.clone(), &mut notes);
    // Nothing turns an invalid request into a billed one: a request that
    // bills in the shape it means runs only as sent.
    if bills(id) {
        return (input, Vec::new());
    }
    let mut request = if request.get("queries").is_some_and(Value::is_array) {
        read_values(tool_name, id, request, &mut notes)
    } else {
        request
    };
    read_integral_floats(id, &mut request, &mut notes);
    read_unpaired_brackets(&mut request, &mut notes);
    (request, notes::per_call(notes))
}

/// Each regex row's `matchString` item that does not compile, but does with
/// its unpaired brackets read as text
/// ([`with_unpaired_as_text`](octocode_engine::search::text_pattern::with_unpaired_as_text)):
/// that reading, said of the item. A literal row has no regex to read.
fn read_unpaired_brackets(request: &mut Value, notes: &mut Vec<Note>) {
    use octocode_engine::search::text_pattern::{validate_text_pattern, with_unpaired_as_text};
    let Some(rows) = request.get_mut("queries").and_then(Value::as_array_mut) else {
        return;
    };
    for (index, row) in rows.iter_mut().enumerate() {
        let pcre2 = match row.get("regexEngine").and_then(Value::as_str) {
            Some("rust") => false,
            Some("pcre2") => true,
            _ => continue,
        };
        let Some(items) = row.get_mut("matchString").and_then(Value::as_array_mut) else {
            continue;
        };
        for (at, item) in items.iter_mut().enumerate() {
            let Some(text) = item.as_str() else {
                continue;
            };
            if validate_text_pattern(text, false, pcre2).valid {
                continue;
            }
            let Some(read) = with_unpaired_as_text(text)
                .filter(|read| validate_text_pattern(read, false, pcre2).valid)
            else {
                continue;
            };
            notes.push((Some(index), format!("matchString[{at}] as `{read}`")));
            *item = Value::String(read);
        }
    }
}

/// Each row's integral floats where an integer is declared
/// ([`integers::read_integral_floats`]).
fn read_integral_floats(id: ToolId, request: &mut Value, notes: &mut Vec<Note>) {
    let Ok(tool) = super::tool_contract(id) else {
        return;
    };
    let Some(rows) = request.get_mut("queries").and_then(Value::as_array_mut) else {
        return;
    };
    for (index, row) in rows.iter_mut().enumerate() {
        integers::read_integral_floats(&tool["querySchema"], index, row, notes);
    }
}

/// Whether running a request bills a classification provider: only a
/// clasify call does.
fn bills(id: ToolId) -> bool {
    id == ToolId::Clasify
}

/// The envelope a request means: a bare row or list of rows wrapped, one
/// row or a text holding the rows for `queries` read as the list. Anything
/// else leaves the shape to validation: a row field beside `queries` is
/// refused, and the refusal says it goes in the rows.
fn shaped(id: ToolId, input: Value, notes: &mut Vec<Note>) -> Value {
    let mut request = match input {
        Value::Array(rows) if !rows.is_empty() && rows.iter().all(Value::is_object) => {
            let mut request = Map::new();
            request.insert("queries".to_owned(), Value::Array(rows));
            notes.push((None, "bare list as {\"queries\":[…]}".to_owned()));
            request
        }
        Value::Object(request) => request,
        other => return other,
    };
    if let Some(queries) = request.get_mut("queries") {
        let rows = match queries {
            Value::Object(_) => Some(Value::Array(vec![queries.take()])),
            Value::String(text) => serde_json::from_str::<Value>(text)
                .ok()
                .filter(Value::is_array),
            _ => None,
        };
        if let Some(rows) = rows {
            *queries = rows;
            notes.push((None, "queries as an array of rows".to_owned()));
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
        let declared_envelope = envelope_fields(id);
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
        notes.push((None, "bare fields as {\"queries\":[{…}]}".to_owned()));
    }
    Value::Object(request)
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

/// The value readings of a shaped request: each issue validation reports
/// that the schema gives one reading ([`read_value`]) is read so, once. A
/// reading can show the next (a discriminator in another letter case hides
/// what its form says of the other fields), so the request as read is
/// validated again and read on until a round reads nothing: every
/// independent reading of a row is made before the row is judged. A row
/// that still fails is judged as read, so its refusal names what is still
/// wrong with it (a selector in another case would otherwise hide the field
/// at fault behind the form it failed to select), and states no note. The
/// whole request stays as sent when its envelope still fails.
fn read_values(tool_name: &str, id: ToolId, input: Value, notes: &mut Vec<Note>) -> Value {
    let Ok((_, mut issues)) = validate_batch(tool_name, input.clone(), false) else {
        return input;
    };
    let Ok(tool) = super::tool_contract(id) else {
        return input;
    };
    let query = &tool["querySchema"];
    let mut read = input.clone();
    let mut made: Vec<Note> = Vec::new();
    loop {
        let before = made.len();
        for issue in issues.rows.iter().flatten().chain(&issues.envelope) {
            if let Some(note) = read_value(query, &mut read, issue) {
                made.push((row_of(&issue.path), note));
            }
        }
        if made.is_empty() {
            return input;
        }
        let Ok((_, after)) = validate_batch(tool_name, read.clone(), false) else {
            return input;
        };
        issues = after;
        if made.len() == before {
            break;
        }
    }
    if !issues.envelope.is_empty() {
        return input;
    }
    // A row that still fails states no note: its refusal is its answer.
    for (index, _) in issues
        .rows
        .iter()
        .enumerate()
        .filter(|(_, issues)| !issues.is_empty())
    {
        made.retain(|(row, _)| *row != Some(index));
    }
    notes.extend(made);
    read
}

/// The row a path lies in (`queries.<n>.…`).
fn row_of(path: &[String]) -> Option<usize> {
    match path {
        [queries, index, _, ..] if queries == "queries" => index.parse().ok(),
        _ => None,
    }
}

/// `path` below its row, or below the request.
fn below_row(path: &[String]) -> &[String] {
    match row_of(path) {
        Some(_) => &path[2..],
        None => path,
    }
}

/// The one reading the schema gives the value `issue` refuses, applied to
/// `input`, and its warning: a value for a list of them or a text holding
/// the declared JSON ([`lenient_value`](super::validate::lenient_value)),
/// an enum literal in another case or separator style, a missing member
/// with a declared default ([`fill_default`]).
fn read_value(query: &Value, input: &mut Value, issue: &ValidationIssue) -> Option<String> {
    if issue.rule_id == "schema.required" {
        return fill_default(query, input, issue);
    }
    let (schema, received) = (issue.schema.as_ref()?, issue.received.as_ref()?);
    let meant = match issue.rule_id.as_str() {
        "schema.type" => super::validate::lenient_value(schema, &query["$defs"], received)?,
        "schema.enum" => super::validate::literal_reading(schema, received)?,
        _ => return None,
    };
    let slot = input.pointer_mut(&crate::json_pointer::of(&issue.path))?;
    if *slot != *received {
        return None;
    }
    let note = notes::reading(&notes::label(below_row(&issue.path)), received, &meant);
    *slot = meant;
    Some(note)
}

/// A member an object sent in a row lacks, whose default the object's schema
/// states (`contextLines: {"after": 1}`, default `{"before": 0, "after": 0}`):
/// omitting the member says what omitting the object says of it, so it is
/// set to that default, and the note says the form. An object whose schemas
/// state no default, or defaults that differ for the member, has no one
/// reading.
fn fill_default(query: &Value, input: &mut Value, issue: &ValidationIssue) -> Option<String> {
    use super::schema_facts::{child_schemas, item_schemas, schema_keyword};
    let [queries, index, within @ .., member] = issue.path.as_slice() else {
        return None;
    };
    if queries != "queries" || within.is_empty() {
        return None;
    }
    index.parse::<usize>().ok()?;
    let defs = &query["$defs"];
    let mut schemas = vec![query];
    for segment in within {
        schemas = schemas
            .into_iter()
            .flat_map(|schema| match segment.parse::<usize>() {
                Ok(_) => item_schemas(schema, defs),
                Err(_) => child_schemas(schema, defs, segment),
            })
            .collect();
    }
    let mut defaults = schemas
        .into_iter()
        .filter_map(|schema| schema_keyword(schema, defs, "default")?.get(member));
    let default = defaults.next()?;
    if defaults.any(|other| other != default) {
        return None;
    }
    let holder = input
        .pointer_mut(&crate::json_pointer::of(
            &issue.path[..issue.path.len() - 1],
        ))?
        .as_object_mut()?;
    if holder.is_empty() || holder.contains_key(member) {
        return None;
    }
    holder.insert(member.clone(), default.clone());
    let label = within.join(".");
    Some(format!(
        "{label}.{member}: {}, its default",
        sketch(default)
    ))
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
    // A cursor sent in the one row of `queries` is the cursor call: a cursor
    // is a whole request, never a query field. The row's other fields stand
    // beside it, read against the query the cursor restores
    // ([`read_beside_cursor`]).
    if let Some(lifted) = row_cursor(&request) {
        let note = "read queries[0].cursor as {\"cursor\":\"…\"} alone".to_owned();
        return (Value::Object(lifted), Some(note));
    }
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

/// A request whose only field is `queries` holding one row with a text
/// `cursor`, as the cursor request: the cursor and the row's other fields
/// beside it.
fn row_cursor(request: &Map<String, Value>) -> Option<Map<String, Value>> {
    let [row] = request.get("queries")?.as_array()?.as_slice() else {
        return None;
    };
    let row = row.as_object()?;
    (request.len() == 1 && row.get("cursor").is_some_and(Value::is_string)).then(|| row.clone())
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

#[cfg(test)]
mod tests;
