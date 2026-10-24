//! The `fields` row pick of the search tools and row `next`/warning helpers.
use crate::response::paging::push_warnings;
use serde_json::{Value, json};

/// `fields` row pick (gh `--json a,b`): the row's `id` keys, then the
/// picked keys in request order, so a picked row stays readable (its
/// number, sha, owner+repo or path). No pick, or a row that is not an
/// object, passes through whole; a key the row lacks stays absent.
pub(crate) fn pick_fields(row: Value, id: &[&str], fields: &[String]) -> Value {
    let Value::Object(map) = row else {
        return row;
    };
    if fields.is_empty() {
        return Value::Object(map);
    }
    let mut picked = serde_json::Map::new();
    for key in id.iter().copied().chain(fields.iter().map(String::as_str)) {
        if !picked.contains_key(key)
            && let Some(value) = map.get(key)
        {
            picked.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(picked)
}

/// A generated `fields` list as plain key names.
pub(crate) fn field_names<T: ToString>(fields: &[T]) -> Vec<String> {
    fields.iter().map(ToString::to_string).collect()
}

/// Makes the row's `next` an object (an empty one when absent), so leads
/// can be added by name.
pub(crate) fn ensure_calls(value: &mut Value) {
    if !value.get("calls").is_some_and(Value::is_object) {
        value["calls"] = json!({});
    }
}

/// Puts one per-call warning first in a row's `warnings` (a rename the
/// rest of the row's warnings follow from).
pub(crate) fn push_warning_front(value: &mut Value, warning: String) {
    match value.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.insert(0, json!(warning)),
        None => value["warnings"] = json!([warning]),
    }
}

/// The one `fields` path of the search tools: picks `value[rows]` (each row
/// keeps its `id` keys) and names picked keys no row carries.
pub(crate) fn pick_rows(value: &mut Value, rows: &str, id: &[&str], fields: &[String]) {
    if fields.is_empty() {
        return;
    }
    if let Some(listed) = value.get_mut(rows).and_then(Value::as_array_mut) {
        for row in listed.iter_mut() {
            *row = pick_fields(std::mem::take(row), id, fields);
        }
    }
    warn_absent_fields(value, rows, fields);
}

/// Names the picked `fields` no row of `value[rows]` carries: a typo, or a
/// value these rows lack. The call still answers; the caller is told
/// instead of reading the missing key as an empty value.
fn warn_absent_fields(value: &mut Value, rows: &str, fields: &[String]) {
    let Some(listed) = value.get(rows).and_then(Value::as_array) else {
        return;
    };
    if listed.is_empty() {
        return;
    }
    let absent = fields
        .iter()
        .map(String::as_str)
        .filter(|key| !listed.iter().any(|row| row.get(*key).is_some()))
        .collect::<Vec<_>>();
    if !absent.is_empty() {
        push_warnings(value, [format!("No row has {}.", absent.join(", "))]);
    }
}

/// Assemble the producer's explicitly typed private calls into public channels.
pub(crate) fn finish_calls(value: &mut Value) {
    if let Some(calls) = value.as_object_mut().and_then(|row| row.remove("calls")) {
        crate::response::result::attach_calls(value, calls);
    }
}
