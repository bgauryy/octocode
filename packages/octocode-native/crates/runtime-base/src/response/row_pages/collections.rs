//! Core-declared evidence collections; all other values stay atomic context.
use super::evidence::DependencyError;
use crate::contracts::output_fields::CollectionRoutes;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn collections<'a>(
    row: &'a Value,
    routes: &CollectionRoutes,
    single: bool,
) -> Vec<(String, &'a Value)> {
    let mut found = found_collections(row, routes, single)
        .into_iter()
        .map(|(pointer, (value, _))| (pointer, value))
        .collect::<Vec<_>>();
    found.sort_by(|(left, _), (right, _)| {
        left.matches('/')
            .count()
            .cmp(&right.matches('/').count())
            .then_with(|| left.cmp(right))
    });
    found
}

/// The depth (`/` count) of a row pointer.
fn depth(pointer: &str) -> usize {
    pointer.matches('/').count()
}

/// Every declared collection `row` holds, by pointer. Selection also carries
/// concrete member/index positions; references preserve those witnesses.
fn found_collections<'a>(
    row: &'a Value,
    routes: &CollectionRoutes,
    single: bool,
) -> BTreeMap<String, (&'a Value, Vec<usize>)> {
    let mut found = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut pending = VecDeque::from([(row, String::new(), 0, Vec::new())]);
    while let Some((value, pointer, state, position)) = pending.pop_front() {
        let Some(route) = routes.nodes.get(state) else {
            continue;
        };
        // References stay at the same value. Only concrete child edges descend;
        // revisiting a state/value pair cannot discover a new collection.
        if !visited.insert((state, pointer.clone())) {
            continue;
        }
        if route.collection && (value.is_array() || value.is_object()) {
            found
                .entry(pointer.clone())
                .or_insert_with(|| (value, position.clone()));
        }
        for reference in &route.references {
            pending.push_back((value, pointer.clone(), *reference, position.clone()));
        }
        let mut push = |key: &str, child: &'a Value, state: usize, ordinal: usize| {
            let mut child_position = position.clone();
            if single {
                child_position.push(ordinal);
            }
            pending.push_back((
                child,
                format!("{pointer}/{}", crate::json_pointer::escape(key)),
                state,
                child_position,
            ));
        };
        match value {
            Value::Object(map) if single && !route.children.is_empty() => {
                // Concrete members prove their order in one pass, without
                // resolving pointers or rescanning a wide object for each key.
                for (ordinal, (key, child)) in map.iter().enumerate() {
                    let named = route.children.get(key);
                    let wildcard = route
                        .children
                        .get("*")
                        .filter(|state| Some(*state) != named);
                    for state in named.into_iter().chain(wildcard) {
                        push(key, child, *state, ordinal);
                    }
                }
            }
            Value::Object(map) if !single => {
                // Other consumers need no positions: keep direct named
                // lookups and inspect other members only for a wildcard.
                for (key, state) in &route.children {
                    if key == "*" {
                        for (key, child) in map {
                            push(key, child, *state, 0);
                        }
                    } else if let Some(child) = map.get(key) {
                        push(key, child, *state, 0);
                    }
                }
            }
            Value::Array(items) if !single || items.len() == 1 => {
                if let Some(state) = route.children.get("*") {
                    for (index, child) in items.iter().enumerate() {
                        push(&index.to_string(), child, *state, index);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// The pointers of the declared collections `row` holds, empty ones included.
pub(super) fn collection_paths(row: &Value, routes: &CollectionRoutes) -> Vec<String> {
    collections(row, routes, false)
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

/// The collection a row too large for its page splits: an outer collection
/// before any inside one of its array elements and, among siblings, the last
/// the row lists. A large sibling listed before it then goes first in parts
/// of its own ([`lean_declared_skeleton`]) and a small one rides the split's
/// first part, so the parts deliver the collections in the row's own order:
/// a file's declarations before the hits that index them.
pub(super) fn collection_pointer(row: &Value, routes: &CollectionRoutes) -> Option<String> {
    let splittable = found_collections(row, routes, true)
        .into_iter()
        .filter(|(_, (value, _))| {
            let count = match value {
                Value::Array(items) => items.len(),
                Value::Object(items) => items.len(),
                _ => 0,
            };
            count > 1
        })
        .collect::<Vec<_>>();
    let outer = splittable.iter().map(|(pointer, _)| depth(pointer)).min()?;
    splittable
        .into_iter()
        .filter(|(pointer, _)| depth(pointer) == outer)
        .max_by(|(_, (_, left)), (_, (_, right))| left.cmp(right))
        .map(|(pointer, _)| pointer)
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
    required: &dyn Fn(&str) -> bool,
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
        if !required(&sibling)
            && let Some(object) = row.pointer_mut(parent).and_then(Value::as_object_mut)
        {
            object.remove(&crate::json_pointer::unescape(key));
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
                    || here.contains(&format!("{holder}/{}", crate::json_pointer::escape(key)))
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
    let key = crate::json_pointer::unescape(key);
    if let Some(object) = part.pointer_mut(parent).and_then(Value::as_object_mut) {
        object.shift_remove(&key);
    }
}

/// The non-empty declared collections disjoint from `pointer` that `row`
/// lists after it, with their pointers.
fn siblings_listed_after(row: &Value, pointer: &str, routes: &CollectionRoutes) -> Vec<String> {
    let positions = found_collections(row, routes, true);
    let Some((_, split)) = positions.get(pointer) else {
        return Vec::new();
    };
    positions
        .iter()
        .filter(|(path, (value, here))| {
            disjoint(path, pointer) && here > split && !emptied(Some(value))
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// A row whose declared sibling the row lists after its split collection,
/// and which outgrows the budget alone, splits in two: the row without that
/// sibling (the split collection's parts, first) and the sibling with the
/// lean context a later part carries (after them). The row's own order then
/// holds: its entries arrive before the paths it lists after them.
pub(super) fn trailing_siblings_split(
    row: &Value,
    budget: usize,
    pointer: &str,
    routes: &CollectionRoutes,
    price: &dyn Fn(&Value) -> Result<usize, DependencyError>,
) -> Result<Option<(Value, Value)>, DependencyError> {
    let mut large = Vec::new();
    for path in siblings_listed_after(row, pointer, routes) {
        if let Some(value) = row.pointer(&path)
            && price(value)?.saturating_mul(4) > budget
        {
            large.push(path);
        }
    }
    if large.is_empty() {
        return Ok(None);
    }
    let clear = |row: &mut Value, path: &str| match row.pointer_mut(path) {
        Some(Value::Array(items)) => items.clear(),
        Some(Value::Object(items)) => items.clear(),
        _ => {}
    };
    let mut head = row.clone();
    for path in &large {
        clear(&mut head, path);
    }
    let mut tail = row.clone();
    for (path, _) in collections(row, routes, false) {
        if !large.contains(&path) {
            clear(&mut tail, &path);
        }
    }
    Ok(Some((head, tail)))
}

/// Put back into `lean`, a row's context thinned for a later part, each
/// non-empty declared collection `source` held that the thinning dropped
/// with the context around it: evidence is never context.
pub(super) fn restore_declared(lean: &mut Value, source: &Value, routes: &CollectionRoutes) {
    for (path, value) in collections(source, routes, false) {
        if emptied(Some(value)) || lean.pointer(&path).is_some() {
            continue;
        }
        let mut node = &mut *lean;
        for segment in path.trim_start_matches('/').split('/') {
            let key = crate::json_pointer::unescape(segment);
            if !node.is_object() {
                break;
            }
            let slot = node.as_object_mut().map(|map| {
                map.entry(key)
                    .or_insert_with(|| Value::Object(Default::default()))
            });
            match slot {
                Some(next) => node = next,
                None => break,
            }
        }
        if let Some(slot) = lean.pointer_mut(&path) {
            *slot = value.clone();
        }
    }
}

/// Large sibling evidence the row lists before the split collection gets its
/// own fragments first; small siblings ride the first part.
pub(super) fn lean_declared_skeleton(
    row: &mut Value,
    budget: usize,
    pointer: &str,
    routes: &CollectionRoutes,
    price: &dyn Fn(&Value) -> Result<usize, DependencyError>,
) -> Result<Option<Value>, DependencyError> {
    let after = siblings_listed_after(row, pointer, routes);
    for (path, value) in collections(row, routes, false) {
        if disjoint(&path, pointer)
            && !after.contains(&path)
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

/// Every declared collection `row` holds with at least one item, at its
/// pointer in the row: a part that holds one of them empty had its items
/// delivered on another part ([`drop_filled_elsewhere`]).
pub(super) fn filled_collections(row: &Value, routes: &CollectionRoutes) -> BTreeSet<String> {
    collections(row, routes, false)
        .into_iter()
        .filter(|(_, value)| !emptied(Some(value)))
        .map(|(path, _)| path)
        .collect()
}

/// Omit from `part`, a part of a split row, each declared collection it
/// holds empty while its row holds it with items (`filled`, in the joined
/// row's indexes: a part that continues element `index` of `array` holds it
/// at index 0), wherever the schema lets a part omit it (`requires`): an
/// empty list beside items delivered on another part would read as an empty
/// section. A collection the row held empty stays `[]`.
pub(super) fn drop_filled_elsewhere(
    part: &mut Value,
    filled: &BTreeSet<String>,
    routes: &CollectionRoutes,
    requires: &dyn Fn(&str) -> bool,
) {
    let continued = part.pointer("/rowPart/continues").and_then(|marker| {
        Some((
            marker.get("array")?.as_str()?.to_owned(),
            marker.get("index")?.as_u64()?,
        ))
    });
    let joined = |path: &str| {
        continued
            .as_ref()
            .and_then(|(array, index)| {
                path.strip_prefix(array.as_str())
                    .and_then(|rest| rest.strip_prefix("/0"))
                    .filter(|rest| rest.is_empty() || rest.starts_with('/'))
                    .map(|rest| format!("{array}/{index}{rest}"))
            })
            .unwrap_or_else(|| path.to_owned())
    };
    let elsewhere = collections(part, routes, false)
        .into_iter()
        .filter(|(path, value)| {
            emptied(Some(value)) && !requires(path) && filled.contains(&joined(path))
        })
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    for path in elsewhere {
        remove_slot(part, &path);
    }
}
