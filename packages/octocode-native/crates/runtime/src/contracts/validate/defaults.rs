//! Applies a tool's observed `defaults` entries to a query.
use serde_json::Value;

pub(super) fn apply_observed_defaults(defaults: &Value, query: &mut Value) {
    let Some(candidates) = defaults.as_array() else {
        return;
    };
    for candidate in candidates {
        let applies = candidate["when"].as_object().is_some_and(|selectors| {
            selectors
                .iter()
                .all(|(field, expected)| query.get(field) == Some(expected))
        }) && candidate["present"].as_array().is_some_and(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .all(|field| query.get(field).is_some())
        }) && candidate["absent"].as_array().is_some_and(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .all(|field| query.get(field).is_none())
        });
        if !applies {
            continue;
        }
        if let Some(values) = candidate["values"].as_object() {
            for (path, value) in values {
                let segments = path.split('.').collect::<Vec<_>>();
                insert_default(query, &segments, value);
            }
        }
        break;
    }
}

fn insert_default(target: &mut Value, path: &[&str], value: &Value) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    let Some(object) = target.as_object_mut() else {
        return;
    };
    if tail.is_empty() {
        object
            .entry((*head).to_owned())
            .or_insert_with(|| value.clone());
        return;
    }
    if let Some(child) = object.get_mut(*head) {
        insert_default(child, tail, value);
    }
}
