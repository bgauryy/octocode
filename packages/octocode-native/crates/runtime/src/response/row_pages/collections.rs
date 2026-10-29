//! Core-declared evidence collections; all other values stay atomic context.
use super::evidence::DependencyError;
use super::skeleton::escape_pointer;
use crate::contracts::output_fields::CollectionRoutes;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn collections<'a>(
    row: &'a Value,
    routes: &CollectionRoutes,
    single: bool,
) -> Vec<(String, &'a Value)> {
    let mut found = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut pending = VecDeque::from([(row, String::new(), 0)]);
    while let Some((value, pointer, state)) = pending.pop_front() {
        let Some(route) = routes.nodes.get(state) else {
            continue;
        };
        // References stay at the same value. Only concrete child edges descend;
        // revisiting a state/value pair cannot discover a new collection.
        if !visited.insert((state, pointer.clone())) {
            continue;
        }
        if route.collection && (value.is_array() || value.is_object()) {
            found.insert(pointer.clone(), value);
        }
        for reference in &route.references {
            pending.push_back((value, pointer.clone(), *reference));
        }
        for (key, child_state) in &route.children {
            if key == "*" {
                match value {
                    Value::Object(map) => {
                        for (key, child) in map {
                            pending.push_back((
                                child,
                                format!("{pointer}/{}", escape_pointer(key)),
                                *child_state,
                            ));
                        }
                    }
                    Value::Array(items) if !single || items.len() == 1 => {
                        for (index, child) in items.iter().enumerate() {
                            pending.push_back((child, format!("{pointer}/{index}"), *child_state));
                        }
                    }
                    _ => {}
                }
            } else if let Some(child) = value.as_object().and_then(|map| map.get(key)) {
                pending.push_back((
                    child,
                    format!("{pointer}/{}", escape_pointer(key)),
                    *child_state,
                ));
            }
        }
    }
    let mut found = found.into_iter().collect::<Vec<_>>();
    found.sort_by(|(left, _), (right, _)| {
        left.matches('/')
            .count()
            .cmp(&right.matches('/').count())
            .then_with(|| left.cmp(right))
    });
    found
}

/// Outer collections are split before entering one of their array elements.
pub(super) fn collection_pointer(row: &Value, routes: &CollectionRoutes) -> Option<String> {
    collections(row, routes, true)
        .into_iter()
        .find_map(|(pointer, value)| {
            let count = match value {
                Value::Array(items) => items.len(),
                Value::Object(items) => items.len(),
                _ => 0,
            };
            (count > 1).then_some(pointer)
        })
}

fn disjoint(left: &str, right: &str) -> bool {
    left != right
        && !left
            .strip_prefix(right)
            .is_some_and(|tail| tail.starts_with('/'))
        && !right
            .strip_prefix(left)
            .is_some_and(|tail| tail.starts_with('/'))
}

/// Only siblings declared as evidence were delivered with the first fragment.
pub(super) fn empty_declared_siblings(row: &mut Value, pointer: &str, routes: &CollectionRoutes) {
    let siblings = collections(row, routes, false)
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| disjoint(path, pointer))
        .collect::<Vec<_>>();
    for sibling in siblings {
        match row.pointer_mut(&sibling) {
            Some(Value::Array(items)) => items.clear(),
            Some(Value::Object(items)) => items.clear(),
            _ => {}
        }
    }
}

pub(super) fn drop_empty_declared_siblings(
    row: &mut Value,
    source: &Value,
    pointer: &str,
    routes: &CollectionRoutes,
    required: &BTreeSet<String>,
) {
    let siblings = collections(row, routes, false)
        .into_iter()
        .filter(|(path, value)| {
            disjoint(path, pointer)
                && match source.pointer(path) {
                    Some(Value::Array(items)) => !items.is_empty(),
                    Some(Value::Object(items)) => !items.is_empty(),
                    _ => false,
                }
                && match value {
                    Value::Array(items) => items.is_empty(),
                    Value::Object(items) => items.is_empty(),
                    _ => false,
                }
        })
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    for sibling in siblings {
        let Some((parent, key)) = sibling.rsplit_once('/') else {
            continue;
        };
        // Page-context requiredness describes direct array-element fields.
        // Slots at a fixed place are [`drop_delivered_elsewhere`]'s.
        let Some((container, _)) = parent.rsplit_once('/') else {
            continue;
        };
        if !source.pointer(container).is_some_and(Value::is_array) {
            continue;
        }
        let key = key.replace("~1", "/").replace("~0", "~");
        if !required.contains(&key)
            && let Some(object) = row.pointer_mut(parent).and_then(Value::as_object_mut)
        {
            object.remove(&key);
        }
    }
}

fn emptied(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Array(items)) => items.is_empty(),
        Some(Value::Object(items)) => items.is_empty(),
        _ => false,
    }
}

/// The fixed collections `row` holds with at least one item: a part that
/// carries one of them empty had it delivered on another part.
pub(super) fn delivered_collections(row: &Value, fixed: &[String]) -> Vec<String> {
    fixed
        .iter()
        .filter(|path| match row.pointer(path) {
            Some(Value::Array(items)) => !items.is_empty(),
            Some(Value::Object(items)) => !items.is_empty(),
            _ => false,
        })
        .cloned()
        .collect()
}

/// Omit from `part` each collection its row delivered on another part, or
/// the whole holder when only context and such emptied collections remain
/// in it, wherever core declares that slot optional. Required slots and
/// collections the row held empty are kept.
pub(super) fn drop_delivered_elsewhere(
    part: &mut Value,
    delivered: &[String],
    optional: &BTreeSet<String>,
) {
    let here = delivered
        .iter()
        .filter(|path| emptied(part.pointer(path)))
        .cloned()
        .collect::<BTreeSet<_>>();
    for path in &here {
        let Some((holder, _)) = path.rsplit_once('/') else {
            continue;
        };
        let context_only = |fields: &serde_json::Map<String, Value>| {
            fields.iter().all(|(key, value)| {
                !(value.is_array() || value.is_object())
                    || here.contains(&format!("{holder}/{}", escape_pointer(key)))
            })
        };
        let whole = optional.contains(holder)
            && part
                .pointer(holder)
                .and_then(Value::as_object)
                .is_some_and(context_only);
        if whole {
            remove_slot(part, holder);
        } else if optional.contains(path) {
            remove_slot(part, path);
        }
    }
}

fn remove_slot(part: &mut Value, pointer: &str) {
    let Some((parent, key)) = pointer.rsplit_once('/') else {
        return;
    };
    let key = key.replace("~1", "/").replace("~0", "~");
    if let Some(object) = part.pointer_mut(parent).and_then(Value::as_object_mut) {
        object.shift_remove(&key);
    }
}

/// Large sibling evidence gets its own fragments; small siblings ride the first.
pub(super) fn lean_declared_skeleton(
    row: &mut Value,
    budget: usize,
    pointer: &str,
    routes: &CollectionRoutes,
    price: &dyn Fn(&Value) -> Result<usize, DependencyError>,
) -> Result<Option<Value>, DependencyError> {
    for (path, value) in collections(row, routes, false) {
        if disjoint(&path, pointer)
            && !matches!(value, Value::Array(items) if items.is_empty())
            && !matches!(value, Value::Object(items) if items.is_empty())
            && price(value)?.saturating_mul(4) > budget
        {
            let rest = row.clone();
            empty_declared_siblings(row, pointer, routes);
            return Ok(Some(rest));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "tests/recursive_routes.rs"]
mod tests;
