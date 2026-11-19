//! Static notices are said once. A tool or decorator writes a note that reads
//! the same on every page and every row of a result under `notices`
//! ([`crate::response::paging::push_notice`]); a continuation page drops its
//! notices, and the stage places each remaining
//! notice on the first row that carries it ([`place`]): in `warnings` on a
//! row with evidence, in `hints.text` on an empty or failed row (where its
//! guidance goes). A notice two or more rows with evidence carry is a
//! warning those rows share: the envelope's `warnings` state it once, so
//! every row has it and none repeats it. No `notices` key reaches a caller.
//! A warning every row of a response states is said once for the call by
//! the hoist stage ([`super::hoist`]).
//!
//! A `partialReasons` code is a fact of its row; what the code means is
//! static, so no response says it: core authors each meaning once and the
//! output reference serves it (`schema <tool> --view output`, and the MCP
//! output resource of the tool).

use serde_json::{Map, Value, json};

use crate::contracts::envelope::NOTICES;

/// Moves every row's notices to where the response states them: a notice
/// two or more rows with evidence carry once in the envelope's `warnings`
/// ([`shared_by_evidence`]), any other on the first row of the response that
/// carries it, where that row states guidance.
pub fn place(structured: &mut Value, rows_key: &str) {
    let Some(rows) = structured.get_mut(rows_key).and_then(Value::as_array_mut) else {
        return;
    };
    let shared = shared_by_evidence(rows);
    let mut said = shared.clone();
    for row in rows {
        let in_hints = guidance_in_hints(row);
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(Value::Array(notices)) = data.remove(NOTICES) else {
            continue;
        };
        let fresh = notices
            .into_iter()
            .filter(|notice| !said.contains(notice))
            .collect::<Vec<_>>();
        if fresh.is_empty() {
            continue;
        }
        said.extend(fresh.iter().cloned());
        if in_hints {
            append(data, "hints", fresh);
        } else {
            append(data, "warnings", fresh);
        }
    }
    if !shared.is_empty()
        && let Some(envelope) = structured.as_object_mut()
    {
        append(envelope, "warnings", shared);
    }
}

/// Whether `row` states its guidance in `hints` (an empty or failed row)
/// rather than in `warnings`.
fn guidance_in_hints(row: &Value) -> bool {
    matches!(
        row["status"].as_str(),
        Some("error" | "empty" | "outOfRange")
    )
}

/// The notices that two or more rows with evidence carry: warnings shared
/// across rows, which the protocol states once for the response (a GitHub
/// rate limit's reset time, which every row that read GitHub needs).
fn shared_by_evidence(rows: &[Value]) -> Vec<Value> {
    let carried = rows
        .iter()
        .filter(|row| !guidance_in_hints(row))
        .filter_map(|row| row["data"].get(NOTICES)?.as_array())
        .collect::<Vec<_>>();
    let mut shared: Vec<Value> = Vec::new();
    for notice in carried.iter().copied().flatten() {
        if !shared.contains(notice)
            && carried
                .iter()
                .filter(|notices| notices.contains(notice))
                .count()
                > 1
        {
            shared.push(notice.clone());
        }
    }
    shared
}

/// States `notes`, facts of the call that answers with `envelope`, in its
/// `warnings`.
pub(crate) fn state_call_notes(envelope: &mut Value, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    if let Some(envelope) = envelope.as_object_mut() {
        append(
            envelope,
            "warnings",
            notes.iter().map(|note| json!(note)).collect(),
        );
    }
}

/// Appends each of `items` not already under `key`, creating the list.
fn append(object: &mut Map<String, Value>, key: &str, items: Vec<Value>) {
    let entry = object.entry(key).or_insert_with(|| json!([]));
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

#[cfg(test)]
mod tests;
