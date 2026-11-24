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
//! - a `matchString` item of a regex row that parses only with its unpaired
//!   brackets read as text (`def route(`): those brackets escaped;
//! - a page size beside a cursor, or a cursor as the one row of `queries`:
//!   the cursor alone.
//!
//! A call says, in one warning, each reading that changed what a value means
//! (a regex bracket read as text, a page size not applied) and bare fields
//! read as one row (the next call can batch); the other readings change no
//! value or meaning and are made silently. A row that
//! still fails is refused as read, for what is still wrong with it. None is applied to a call that bills. No field is
//! read under another name, moved into another object or inferred: an
//! undeclared field is refused, and the refusal names the declared one.

use serde_json::{Map, Value};

use super::validate::{ValidationIssue, validate_batch};
use crate::id::ToolId;

mod beside_cursor;
mod integers;
mod notes;
pub use beside_cursor::{BesideCursor, drop_unopened_page_size, read_beside_cursor};

/// One lenient reading, said of the row it was made on (`None`: the
/// request): the field and how it was read ([`notes::per_call`] joins them).
type Note = (Option<usize>, String);

/// `input` as the contract takes it, with one warning naming the readings
/// that changed what a value means (a regex bracket read as text, a page size
/// not applied) and bare fields read as one row; a reading that changes no
/// value or meaning (a bare list or a `queries` object or text, JSON text, a one-item list, an integral float, a letter case, an
/// ordered pair, a cursor in its row) is made silently. Otherwise unchanged,
/// so validation names what is wrong with what was sent.
pub fn normalized_input(tool_name: &str, input: Value) -> (Value, Vec<String>) {
    let (input, said, _quiet) = normalize(tool_name, input);
    (input, notes::per_call(said))
}

/// Every reading [`normalized_input`] makes, the silent ones included: what
/// tests of a reading's form check.
#[cfg(test)]
pub(crate) fn normalized_readings(tool_name: &str, input: Value) -> (Value, Vec<String>) {
    let (input, mut said, quiet) = normalize(tool_name, input);
    said.extend(quiet);
    (input, notes::per_call(said))
}

/// The request as read, the readings it states, and the ones it makes silently.
fn normalize(tool_name: &str, input: Value) -> (Value, Vec<Note>, Vec<Note>) {
    let Some(id) = ToolId::from_name(tool_name) else {
        return (input, Vec::new(), Vec::new());
    };
    let (input, resumed) = cursor_alone(input);
    if let Some((note, said)) = resumed {
        let note = vec![(None, note)];
        return if said {
            (input, note, Vec::new())
        } else {
            (input, Vec::new(), note)
        };
    }
    let (mut said, mut quiet) = (Vec::new(), Vec::new());
    let mut input = input;
    if id != ToolId::Clasify {
        drop_brief(&mut input, &mut said);
    }
    let request = shaped(id, input.clone(), &mut said, &mut quiet);
    // Nothing turns an invalid request into a billed one: a request that
    // bills in the shape it means runs only as sent.
    if bills(id) {
        return (input, Vec::new(), Vec::new());
    }
    let mut request = if request.get("queries").is_some_and(Value::is_array) {
        read_values(tool_name, id, request, &mut quiet)
    } else {
        request
    };
    read_integral_floats(id, &mut request, &mut quiet);
    read_unpaired_brackets(&mut request, &mut said);
    (request, said, quiet)
}

/// The brief fields ([`BRIEF_FIELDS`](super::BRIEF_FIELDS)) a tool other
/// than clasify does not declare, sent beside the request or in a row:
/// dropped, since no form or result depends on them, and said, so a caller
/// that still sends them stops.
fn drop_brief(request: &mut Value, said: &mut Vec<Note>) {
    let mut drop_from = |row: &mut Value, at: Option<usize>| {
        let Some(fields) = row.as_object_mut() else {
            return;
        };
        let gone: Vec<&str> = super::BRIEF_FIELDS
            .into_iter()
            .filter(|field| fields.shift_remove(*field).is_some())
            .collect();
        if !gone.is_empty() {
            said.push((
                at,
                format!("{} dropped: only clasify takes them", gone.join(", ")),
            ));
        }
    };
    drop_from(request, None);
    if let Some(rows) = request.get_mut("queries").and_then(Value::as_array_mut) {
        for (index, row) in rows.iter_mut().enumerate() {
            drop_from(row, Some(index));
        }
    }
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
fn shaped(id: ToolId, input: Value, said: &mut Vec<Note>, notes: &mut Vec<Note>) -> Value {
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
    // An empty `queries` beside row fields holds no row: the bare fields are
    // its one reading (2026-11-24: workflow W3's first call, 3 of 3 runs).
    if request
        .get("queries")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
        && request
            .keys()
            .any(|field| field != "queries" && super::query_declares(id, field))
    {
        request.remove("queries");
    }
    drop_repeated_row_fields(id, &mut request);
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
        // A cursor call is the cursor alone (a page size sent beside it is
        // dropped once the cursor opens); a query field beside it is read against
        // the cursor's saved query ([`read_beside_cursor`]).
        return Value::Object(request);
    } else {
        // A field neither the rows nor the envelope declare travels with the
        // row, so its refusal names it in queries[0] beside that row's own
        // repair; an object with no row field stays as sent.
        let declared_envelope = envelope_fields(id);
        let (envelope, row): (Map<_, _>, Map<_, _>) =
            request.into_iter().partition(|(field, _)| {
                !super::query_declares(id, field)
                    && declared_envelope.iter().any(|known| known == field)
            });
        if !row.keys().any(|field| super::query_declares(id, field)) {
            let mut unchanged = row;
            unchanged.extend(envelope);
            return Value::Object(unchanged);
        }
        request = envelope;
        request.insert("queries".to_owned(), Value::Array(vec![Value::Object(row)]));
        // Said, unlike the other envelope readings: bare fields hold one row,
        // so a caller that keeps sending them never batches (2026-11-24: 81%
        // of ghSearchCode calls were bare once this went silent, 31% before).
        said.push((
            None,
            "bare fields as {\"queries\":[{…}]}, one row: queries batches several".to_owned(),
        ));
    }
    Value::Object(request)
}

/// Drop a row field sent beside `queries` that every row already holds with
/// the same value: the copy says nothing the rows do not. Any other row field
/// beside `queries` stays for validation to refuse.
fn drop_repeated_row_fields(id: ToolId, request: &mut Map<String, Value>) {
    let Some(rows) = request.get("queries").and_then(Value::as_array) else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    let envelope = envelope_fields(id);
    let repeated: Vec<String> = request
        .iter()
        .filter(|(field, _)| {
            *field != "queries"
                && super::query_declares(id, field)
                && !envelope.iter().any(|known| known == *field)
        })
        .filter(|(field, value)| {
            rows.iter()
                .all(|row| row.get(field.as_str()) == Some(value))
        })
        .map(|(field, _)| field.clone())
        .collect();
    for field in repeated {
        request.remove(&field);
    }
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
/// or an enum literal in another case or separator style. A member an
/// object requires is never filled from the object's default: that default
/// says what omitting the whole object means, and the member can decide
/// what the others mean (`sort:{"order":"desc"}` lacks the key it ranks by).
fn read_value(query: &Value, input: &mut Value, issue: &ValidationIssue) -> Option<String> {
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

/// The page size a request sets; a cursor carries the one its walk was cut at.
const PAGE_SIZE: &str = "responseChars";

/// A cursor call sent as a row: the row's cursor lifted to the request. A
/// page size beside a cursor is read once the cursor opens, against the walk
/// it was issued for ([`beside_cursor::drop_lone_page_size`]). A query field
/// beside a cursor is read against the query the cursor restores
/// ([`read_beside_cursor`]); validation refuses any other.
fn cursor_alone(input: Value) -> (Value, Option<(String, bool)>) {
    let request = match input {
        Value::Object(request) => request,
        other => return (other, None),
    };
    // A cursor sent in the one row of `queries` is the cursor call: a cursor
    // is a whole request, never a query field. The row's other fields stand
    // beside it, read against the query the cursor restores
    // ([`read_beside_cursor`]).
    if let Some(lifted) = row_cursor(&request) {
        let note = "queries[0].cursor as {\"cursor\":\"…\"} alone".to_owned();
        return (Value::Object(lifted), Some((note, false)));
    }
    (Value::Object(request), None)
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
