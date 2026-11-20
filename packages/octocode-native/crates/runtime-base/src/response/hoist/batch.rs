//! F4: what every result row of a batch states alike is a fact of the call,
//! stated once in the envelope; each row keeps what is its own.
//! - Warnings every row states word for word go to the envelope's
//!   `warnings` ([`warnings`]).
//! - Keyed facts: a later result whose `by` (a snapshot id) an earlier one
//!   shares omits each fact equal to the one last stated for that `by`
//!   ([`keyed`]); `by` stays on every result, so each names what it read.
//! - Row fields the plan lists (identity scalars, disclosure objects) that
//!   every row states alike go to the envelope's `shared.data`, at their row
//!   path ([`shared`]); a verdict or evidence is never listed.
//! - Hints every empty or failed row states alike go to the envelope's
//!   `hints` ([`hints`]).
//! - Single-query `related` calls of one tool, from two or more rows, merge
//!   into one envelope call holding each row's query ([`related`]); it runs
//!   unchanged as each of them would.
//!
//! Nothing is dropped: a value some rows lack stays on the rows that state
//! it, and a one-row response moves nothing but keyed facts.
use super::plan::{Batch, Keyed};
use serde_json::{Map, Value, json};

/// States once what `batch` lets the rows under `structured["results"]`
/// share.
pub(crate) fn apply(batch: &Batch, structured: &mut Value) {
    warnings(structured);
    if let Some(results) = structured.get_mut("results").and_then(Value::as_array_mut) {
        keyed(results, &batch.keyed);
    }
    shared(structured, batch);
    if batch.hints {
        hints(structured);
    }
    if batch.related {
        related(structured);
    }
}

/// A warning every row of a response states word for word is said once, in
/// the envelope's `warnings`, and each row keeps the warnings that are its
/// own.
pub(crate) fn warnings(structured: &mut Value) {
    let lists = rows(structured)
        .map(|row| list(row, "/data/warnings"))
        .collect::<Vec<_>>();
    let shared = common(&lists);
    if shared.is_empty() {
        return;
    }
    for data in datas(structured) {
        strip(data, "warnings", &shared);
    }
    append(structured, "warnings", shared);
}

/// Hints every empty or failed row states word for word, when two or more
/// rows are empty or failed, are said once in the envelope's `hints`.
fn hints(structured: &mut Value) {
    let lists = rows(structured)
        .filter(|row| guidance_row(row))
        .map(|row| list(row, "/data/hints"))
        .collect::<Vec<_>>();
    let shared = common(&lists);
    if shared.is_empty() {
        return;
    }
    for row in structured["results"].as_array_mut().into_iter().flatten() {
        if guidance_row(row)
            && let Some(data) = row.get_mut("data").and_then(Value::as_object_mut)
        {
            strip(data, "hints", &shared);
        }
    }
    append(structured, "hints", shared);
}

/// Whether `row` states its guidance in `hints`: an empty or failed row.
fn guidance_row(row: &Value) -> bool {
    matches!(
        row["status"].as_str(),
        Some("error" | "empty" | "outOfRange")
    )
}

/// The fields `batch.shared` lists that two or more rows all state with one
/// value, moved to the envelope's `shared.data` at their row path.
fn shared(structured: &mut Value, batch: &Batch) {
    for pattern in &batch.shared {
        let path = pattern.tokens();
        let pointer = crate::json_pointer::of(path);
        let values = rows(structured)
            .map(|row| row.get("data").and_then(|data| data.pointer(&pointer)))
            .collect::<Vec<_>>();
        let Some(Some(first)) = values.first().copied() else {
            continue;
        };
        if values.len() < 2 || values.iter().any(|value| *value != Some(first)) {
            continue;
        }
        let first = first.clone();
        for row in structured["results"].as_array_mut().into_iter().flatten() {
            if let Some(data) = row.get_mut("data") {
                super::omit::remove(data, path);
            }
        }
        let mut at = &mut structured["shared"]["data"];
        for token in &path[..path.len().saturating_sub(1)] {
            at = &mut at[token.as_str()];
        }
        if let Some(last) = path.last() {
            at[last.as_str()] = first;
        }
    }
}

/// Keyed facts ([`Keyed`]) over `results` in batch order.
pub(crate) fn keyed(results: &mut [Value], groups: &[Keyed]) {
    for group in groups {
        // Per `by` value and field, the value last stated.
        let mut stated: Vec<(Value, &str, Value)> = Vec::new();
        for result in results.iter_mut() {
            if result["status"] == "error" {
                continue;
            }
            let Some(by) = result
                .pointer(&group.by)
                .filter(|by| !by.is_null())
                .cloned()
            else {
                continue;
            };
            for field in &group.fields {
                let Some(value) = result.pointer(field).cloned() else {
                    continue;
                };
                match stated
                    .iter_mut()
                    .find(|(read, name, _)| *read == by && *name == field.as_str())
                {
                    Some((_, _, last)) if *last == value => remove_pointer(result, field),
                    Some((_, _, last)) => *last = value,
                    None => stated.push((by.clone(), field, value)),
                }
            }
        }
    }
}

/// Single-query `related` calls of one tool and input, offered by two or
/// more rows, become one envelope call holding each row's query, in row
/// order (a query two rows offer once), as many per call as the tool takes.
/// An empty or failed row's calls are its recovery, which its hints name:
/// they merge only with other such rows' and only when every one of those
/// rows offers the same queries, so the envelope call is each row's own
/// recovery stated once; a row whose recovery differs keeps it beside it.
fn related(structured: &mut Value) {
    let recovers = rows(structured).map(guidance_row).collect::<Vec<_>>();
    let mut merged = Vec::new();
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for recovery in [false, true] {
        let offers = offers(structured, |row| recovers[row] == recovery);
        merge_offers(&offers, recovery, &mut merged, &mut taken);
    }
    if merged.is_empty() {
        return;
    }
    taken.sort_unstable();
    for (row, at) in taken.into_iter().rev() {
        let data = &mut structured["results"][row]["data"];
        if let Some(own) = data.get_mut("related").and_then(Value::as_array_mut) {
            own.remove(at);
            if own.is_empty()
                && let Some(data) = data.as_object_mut()
            {
                data.shift_remove("related");
            }
        }
    }
    append(structured, "related", merged);
}

/// One row's single-query `related` call: (row, position in its `related`,
/// the call without its query, query).
type Offer = (usize, usize, Value, Value);

/// The single-query `related` calls of the rows `of` admits.
fn offers(structured: &Value, of: impl Fn(usize) -> bool) -> Vec<Offer> {
    let mut offers = Vec::new();
    for (row, result) in rows(structured).enumerate().filter(|(row, _)| of(*row)) {
        for (at, call) in list(result, "/data/related").into_iter().enumerate() {
            let single = call
                .pointer("/input/queries")
                .and_then(Value::as_array)
                .filter(|queries| queries.len() == 1)
                .map(|queries| queries[0].clone());
            if let Some(query) = single {
                let mut shape = call;
                shape["input"]["queries"] = json!([]);
                offers.push((row, at, shape, query));
            }
        }
    }
    offers
}

/// [`related`] over one group of rows' `offers`: each shape two or more rows
/// offer becomes envelope calls (pushed to `merged`, the offers it takes to
/// `taken`); for `recovery` rows only when every row offers the same queries.
fn merge_offers(
    offers: &[Offer],
    recovery: bool,
    merged: &mut Vec<Value>,
    taken: &mut Vec<(usize, usize)>,
) {
    let mut shapes: Vec<&Value> = Vec::new();
    for (_, _, shape, _) in offers {
        if !shapes.contains(&shape) {
            shapes.push(shape);
        }
    }
    for shape in shapes {
        let held = offers
            .iter()
            .filter(|(_, _, offered, _)| offered == shape)
            .collect::<Vec<_>>();
        let mut from = held.iter().map(|(row, ..)| *row).collect::<Vec<_>>();
        from.dedup();
        let per_call = shape["tool"]
            .as_str()
            .and_then(crate::id::ToolId::from_name)
            .map_or(1, crate::response::result::rows_per_call);
        if from.len() < 2 || per_call < 2 {
            continue;
        }
        let asked = |row: usize| {
            let mut queries = held
                .iter()
                .filter(|(of, ..)| *of == row)
                .map(|(.., query)| query.to_string())
                .collect::<Vec<_>>();
            queries.sort_unstable();
            queries.dedup();
            queries
        };
        if recovery && from.iter().any(|&row| asked(row) != asked(from[0])) {
            continue;
        }
        let mut queries: Vec<Value> = Vec::new();
        for (row, at, _, query) in &held {
            taken.push((*row, *at));
            if !queries.contains(query) {
                queries.push(query.clone());
            }
        }
        for chunk in queries.chunks(per_call) {
            let mut call = shape.clone();
            call["input"]["queries"] = Value::Array(chunk.to_vec());
            merged.push(call);
        }
    }
}

fn rows(structured: &Value) -> impl Iterator<Item = &Value> {
    structured["results"].as_array().into_iter().flatten()
}

fn datas(structured: &mut Value) -> impl Iterator<Item = &mut Map<String, Value>> {
    structured["results"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter_map(|row| row.get_mut("data").and_then(Value::as_object_mut))
}

fn list(row: &Value, pointer: &str) -> Vec<Value> {
    row.pointer(pointer)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The items every one of two or more `lists` holds, in the first's order.
fn common(lists: &[Vec<Value>]) -> Vec<Value> {
    let Some((first, rest)) = lists.split_first().filter(|(_, rest)| !rest.is_empty()) else {
        return Vec::new();
    };
    first
        .iter()
        .filter(|item| rest.iter().all(|list| list.contains(item)))
        .cloned()
        .collect()
}

/// Removes `shared` from `data[key]`, and the list once it is empty.
fn strip(data: &mut Map<String, Value>, key: &str, shared: &[Value]) {
    if let Some(Value::Array(own)) = data.get_mut(key) {
        own.retain(|item| !shared.contains(item));
        if own.is_empty() {
            data.shift_remove(key);
        }
    }
}

/// Appends each of `items` not already under the envelope's `key`.
fn append(structured: &mut Value, key: &str, items: Vec<Value>) {
    let Some(envelope) = structured.as_object_mut() else {
        return;
    };
    let entry = envelope.entry(key).or_insert_with(|| json!([]));
    if !entry.is_array() {
        *entry = json!([]);
    }
    if let Value::Array(list) = entry {
        for item in items {
            if !list.contains(&item) {
                list.push(item);
            }
        }
    }
}

/// Removes the member `pointer` names from `value`.
fn remove_pointer(value: &mut Value, pointer: &str) {
    let Some((parent, key)) = pointer.rsplit_once('/') else {
        return;
    };
    if let Some(object) = value.pointer_mut(parent).and_then(Value::as_object_mut) {
        object.shift_remove(&crate::json_pointer::unescape(key));
    }
}
