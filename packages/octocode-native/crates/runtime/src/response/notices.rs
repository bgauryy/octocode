//! Static notices are said once. A tool or decorator writes a note that reads
//! the same on every page and every row of a result under `notices`
//! ([`crate::response::paging::push_notice`]); a continuation page drops its
//! notices, and the stage places each remaining
//! notice on the first row that carries it ([`place`]): in `warnings` on a
//! row with evidence, in `hints.text` on an empty or failed row (where its
//! guidance goes). No `notices` (or `statedReasons`) key reaches a caller. A warning every row
//! of a response states is said once for the call
//! ([`state_shared_warnings`]).

use serde_json::{Map, Value, json};

use crate::response::paging::{NOTICES, STATED_REASONS, TIMED_NOTICES};

/// Each coverage reason a row carries, as a notice saying what it means: core
/// authors the sentence once, and it is sent only when this response has that
/// gap ([`place`] says it on the first row that carries it), and only where
/// the row's own text does not already state it
/// ([`crate::response::paging::mark_stated`]).
pub(crate) fn explain_codes(structured: &mut Value, rows_key: &str) {
    let Some(rows) = structured.get_mut(rows_key).and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let Some(data) = row.get_mut("data") else {
            continue;
        };
        let codes = data["partialReasons"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(data.pointer("/coverage/reason"))
            .filter_map(Value::as_str)
            .filter(|code| {
                !data[STATED_REASONS]
                    .as_array()
                    .is_some_and(|stated| stated.iter().any(|stated| stated == code))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for code in codes {
            if let Some(meaning) = crate::contracts::partial_reason_meaning(&code) {
                crate::response::paging::push_notice(data, format!("{code}: {meaning}"));
            }
        }
    }
}

/// Moves every row's notices to where the row states guidance, keeping
/// each notice on the first row of the response that carries it.
pub(crate) fn place(structured: &mut Value, rows_key: &str) {
    let Some(rows) = structured.get_mut(rows_key).and_then(Value::as_array_mut) else {
        return;
    };
    let mut said: Vec<Value> = Vec::new();
    for row in rows {
        let guidance_in_hints = matches!(
            row["status"].as_str(),
            Some("error" | "empty" | "outOfRange")
        );
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        data.remove(STATED_REASONS);
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
        if guidance_in_hints {
            append(data, "hints", fresh);
        } else {
            append(data, "warnings", fresh);
        }
    }
}

/// A warning every row of a response states word for word is a fact of the
/// call, not of one row: it is said once, in the envelope's `warnings`, and
/// each row keeps the warnings that are its own. Nothing is dropped: a
/// warning some rows lack stays on the rows that state it, and a one-row
/// response repeats nothing.
pub(crate) fn state_shared_warnings(structured: &mut Value, rows_key: &str) {
    let Some(rows) = structured.get(rows_key).and_then(Value::as_array) else {
        return;
    };
    let warnings = |row: &Value| {
        row.pointer("/data/warnings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let Some((first, rest)) = rows.split_first().filter(|(_, rest)| !rest.is_empty()) else {
        return;
    };
    let mut shared = warnings(first);
    for row in rest {
        let stated = warnings(row);
        shared.retain(|warning| stated.contains(warning));
    }
    if shared.is_empty() {
        return;
    }
    for row in structured[rows_key].as_array_mut().into_iter().flatten() {
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(Value::Array(own)) = data.get_mut("warnings") {
            own.retain(|warning| !shared.contains(warning));
            if own.is_empty() {
                data.remove("warnings");
            }
        }
    }
    if let Some(envelope) = structured.as_object_mut() {
        append(envelope, "warnings", shared);
    }
}

/// A measured wait ([`crate::response::paging::push_timed_notice`]) is a fact
/// of the call, not of the row that measured it nor of the result: rows of
/// one call wait on the same server for different spans, and a later page of
/// the result is another call, which waited for none of them. So the stage
/// takes them out of the rows (no row keeps the key) and returns each
/// distinct text once, with the longest span any row measured; the call that
/// measured them states them on its own response ([`state_call_notes`]).
pub(crate) fn take_timed_notices(structured: &mut Value, rows_key: &str) -> Vec<String> {
    let mut longest: Vec<(String, f64)> = Vec::new();
    for row in structured
        .get_mut(rows_key)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(Value::Array(timed)) = row
            .get_mut("data")
            .and_then(Value::as_object_mut)
            .and_then(|data| data.remove(TIMED_NOTICES))
        else {
            continue;
        };
        for notice in timed {
            let (Some(text), Some(seconds)) = (notice["text"].as_str(), notice["seconds"].as_f64())
            else {
                continue;
            };
            match longest.iter_mut().find(|(seen, _)| seen == text) {
                Some((_, kept)) => *kept = kept.max(seconds),
                None => longest.push((text.to_owned(), seconds)),
            }
        }
    }
    longest
        .into_iter()
        .map(|(text, seconds)| text.replacen("{}", &format!("{seconds:.1} s"), 1))
        .collect()
}

/// States `notes`, facts of the call that answers with `envelope`
/// ([`take_timed_notices`]), in its `warnings`.
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
