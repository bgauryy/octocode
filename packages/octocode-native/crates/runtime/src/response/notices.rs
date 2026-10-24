//! Static notices are said once. A tool or decorator writes a note that reads
//! the same on every page and every row of a result under `notices`
//! ([`crate::response::paging::push_notice`]); a continuation page drops its
//! notices ([`drop_on_later_page`]), and the stage places each remaining
//! notice on the first row that carries it ([`place`]): in `warnings` on a
//! row with evidence, in `hints.text` on an empty or failed row (where its
//! guidance goes). No `notices` key reaches a caller.

use serde_json::{Map, Value, json};

use crate::response::paging::NOTICES;

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
