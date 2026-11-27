//! Row values read as absent: an empty text or object sent for an optional
//! field (a model filling every slot), and the fields a row's form takes no
//! meaning from (`unique` outside `view:"matchOnly"`, `sort` on a tree).

use serde_json::{Map, Value};

use super::Note;
use crate::contracts::validate::validate_batch;

/// One ignored field: its tool, its name, why the row takes no meaning from
/// it, and whether a row's form ignores it.
type Ignored = (
    &'static str,
    &'static str,
    &'static str,
    fn(&Map<String, Value>) -> bool,
);

/// A field a row's form gives no meaning, and when: `unique` ranks the
/// matches of `view:"matchOnly"` alone; a tree has no order to sort.
const IGNORED: [Ignored; 2] = [
    ("localSearch", "unique", "it needs view:matchOnly", |row| {
        row.get("view").and_then(Value::as_str) != Some("matchOnly")
    }),
    ("structureSearch", "sort", "a tree has no sort", |row| {
        row.get("operation").and_then(Value::as_str) == Some("tree")
    }),
];

/// Whether `value` is an empty text or an empty object.
fn is_empty(value: &Value) -> bool {
    match value {
        Value::String(text) => text.is_empty(),
        Value::Object(fields) => fields.is_empty(),
        _ => false,
    }
}

/// The rows of `request`'s `queries`, when it holds a list.
fn rows(request: &mut Value) -> Option<&mut Vec<Value>> {
    request.get_mut("queries")?.as_array_mut()
}

/// Each row's ignored field ([`IGNORED`]) removed, said of its row.
pub(super) fn drop_ignored(tool_name: &str, request: &mut Value, said: &mut Vec<Note>) {
    let Some(rows) = rows(request) else {
        return;
    };
    for (index, row) in rows.iter_mut().enumerate() {
        let Some(fields) = row.as_object_mut() else {
            continue;
        };
        for (tool, field, why, ignored) in IGNORED {
            if tool == tool_name && fields.contains_key(field) && ignored(fields) {
                fields.shift_remove(field);
                said.push((Some(index), format!("{field} dropped: {why}")));
            }
        }
    }
}

/// Each row's empty text or object removed, silently: it says what an absent
/// field says. A field the row's form requires stays, so validation still
/// names it.
pub(super) fn drop_empty_optionals(tool_name: &str, request: &mut Value) {
    let Some(rows) = rows(request) else {
        return;
    };
    let mut removed: Vec<(usize, String, Value)> = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        let Some(fields) = row.as_object_mut() else {
            continue;
        };
        let empty: Vec<String> = fields
            .iter()
            .filter(|(_, value)| is_empty(value))
            .map(|(field, _)| field.clone())
            .collect();
        for field in empty {
            if let Some(value) = fields.shift_remove(&field) {
                removed.push((index, field, value));
            }
        }
    }
    if removed.is_empty() {
        return;
    }
    let refused = |request: &Value, index: usize| {
        validate_batch(tool_name, request.clone(), false)
            .ok()
            .map(|(_, issues)| issues.rows.get(index).map_or(0, Vec::len))
    };
    let mut sent = request.clone();
    restore(&mut sent, removed.clone());
    // A row keeps its removal only when it is refused less for it: an empty
    // value a form requires is refused either way, so it stays as sent.
    let (kept, needed): (Vec<_>, Vec<_>) = removed.into_iter().partition(|(index, _, _)| {
        matches!((refused(&sent, *index), refused(request, *index)), (Some(before), Some(after)) if after < before)
    });
    drop(kept);
    restore(request, needed);
}

/// Put each removed field back in its row.
fn restore(request: &mut Value, removed: Vec<(usize, String, Value)>) {
    let Some(rows) = rows(request) else {
        return;
    };
    for (index, field, value) in removed {
        if let Some(fields) = rows.get_mut(index).and_then(Value::as_object_mut) {
            fields.insert(field, value);
        }
    }
}
