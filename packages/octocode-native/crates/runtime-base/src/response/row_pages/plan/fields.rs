//! A planned row's set-aside fields: its first-fragment fields and its
//! guidance, taken out before packing and restored on the parts that carry them.
use crate::response::result::{HINTS_KEY, PAGES_KEY};
use serde_json::{Map, Value, json};

/// Move the row's guidance (`next`, `hints`, `related`, on the row and on
/// its `data`) out, in the shape [`restore_fields`] puts back.
pub(super) fn take_guidance(row: &mut Value) -> Map<String, Value> {
    let mut guidance = Map::new();
    let take = |slot: Option<&mut Map<String, Value>>| {
        let mut taken = Map::new();
        if let Some(slot) = slot {
            for key in [PAGES_KEY, HINTS_KEY, "related"] {
                if let Some(value) = slot.shift_remove(key) {
                    taken.insert(key.to_owned(), value);
                }
            }
        }
        taken
    };
    let data = take(row.get_mut("data").and_then(Value::as_object_mut));
    let own = take(row.as_object_mut());
    guidance.extend(own);
    if !data.is_empty() {
        guidance.insert("data".into(), Value::Object(data));
    }
    guidance
}

pub(super) fn restore_fields(row: &mut Map<String, Value>, fields: Map<String, Value>) {
    for (key, value) in fields {
        match (row.get_mut(&key), value) {
            (Some(Value::Object(target)), Value::Object(fields)) => restore_fields(target, fields),
            (_, value) => {
                row.insert(key, value);
            }
        }
    }
}

pub(super) fn insert_field(target: &mut Map<String, Value>, path: &[String], value: Value) {
    let Some((key, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        target.insert(key.clone(), value);
        return;
    }
    if let Value::Object(nested) = target.entry(key.clone()).or_insert_with(|| json!({})) {
        insert_field(nested, rest, value);
    }
}
