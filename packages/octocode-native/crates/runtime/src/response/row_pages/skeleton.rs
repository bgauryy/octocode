use crate::id::continuation_channels::{HINTS_KEY, PAGES_KEY};
use serde_json::{Map, Value, json};

/// Page-plan size reserved for a row's per-call facts. It depends only on
/// their shape, so executions that differ in the values plan identical pages.
pub(super) fn volatile_reserve(fields: &Map<String, Value>) -> usize {
    fn shape(value: &Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, child)| (key.clone(), shape(child)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(shape).collect()),
            _ => json!("x".repeat(40)),
        }
    }
    let shaped: Map<String, Value> = fields
        .iter()
        .map(|(key, child)| (key.clone(), shape(child)))
        .collect();
    octocode_engine::json_size::utf16_units(&shaped).unwrap_or(0) - 1
}

fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The largest evidence array with at least two elements reachable through
/// objects and single-element arrays. Executable calls remain indivisible.
fn largest_array(value: &Value, pointer: &str) -> Option<(String, usize)> {
    let mut best: Option<(String, usize)> = None;
    let mut consider = |candidate: Option<(String, usize)>| {
        if let Some(candidate) = candidate
            && best.as_ref().is_none_or(|current| candidate.1 > current.1)
        {
            best = Some(candidate);
        }
    };
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == PAGES_KEY || key == HINTS_KEY {
                    continue;
                }
                consider(largest_array(
                    child,
                    &format!("{pointer}/{}", escape_pointer(key)),
                ));
            }
        }
        Value::Array(items) if items.len() >= 2 => {
            consider(Some((
                pointer.to_owned(),
                octocode_engine::json_size::utf16_units(value).unwrap_or(0),
            )));
        }
        Value::Array(items) => {
            if let Some(item) = items.first() {
                consider(largest_array(item, &format!("{pointer}/0")));
            }
        }
        _ => {}
    }
    best
}

/// Empty every remaining array of `skeleton` large enough (over a quarter
/// of `budget`) to matter if repeated per fragment. Returns, when any was
/// emptied, the split's rest: the skeleton as it was, holding them and the
/// lists beside the paged array at `pointer`. The rest is the split's first
/// part, so its slices carry those lists emptied.
pub(super) fn lean_skeleton(skeleton: &mut Value, budget: usize, pointer: &str) -> Option<Value> {
    let mut rest = None;
    while let Some((relative, chars)) = skeleton
        .get("data")
        .and_then(|data| largest_array(data, ""))
        && chars.saturating_mul(4) > budget
    {
        if rest.is_none() {
            rest = Some(skeleton.clone());
        }
        let path = format!("/data{relative}");
        match skeleton.pointer_mut(&path).and_then(Value::as_array_mut) {
            Some(items) => items.clear(),
            None => break,
        }
    }
    if rest.is_some() {
        empty_secondary_lists(skeleton, pointer);
    }
    rest
}

/// The outermost array element a paged array at `pointer` lies in (reached
/// through single-element arrays), as (array pointer, element index).
pub(super) fn enclosing_element(value: &Value, pointer: &str) -> Option<(String, usize)> {
    let mut prefix = String::new();
    let mut node = value;
    for segment in pointer.trim_start_matches('/').split('/') {
        let key = segment.replace("~1", "/").replace("~0", "~");
        if node.is_array() {
            return key.parse().ok().map(|index| (prefix, index));
        }
        node = node.as_object()?.get(&key)?;
        prefix = format!("{prefix}/{segment}");
    }
    None
}

/// Serialized length a `rowPart.continues` marker adds.
pub(super) fn continues_chars(array: &str, index: usize) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"continues": {"array": array, "index": index}}))
        .unwrap_or(0)
        - 1
}

/// Move the largest array out of `row`'s data, as (JSON pointer, items).
pub(super) fn take_largest_array(row: &mut Value) -> Option<(String, Vec<Value>)> {
    let (relative, _) = row.get("data").and_then(|data| largest_array(data, ""))?;
    let pointer = format!("/data{relative}");
    let items = row
        .pointer_mut(&pointer)
        .and_then(Value::as_array_mut)
        .map(std::mem::take)?;
    Some((pointer, items))
}

/// Resolve declared collection paths without entering arbitrary answer values
/// or treating coordinate tuples and criteria as evidence lists.
pub(super) fn collection_pointer(row: &Value, paths: &[&str]) -> Option<String> {
    fn visit(value: &Value, segments: &[&str], pointer: &str) -> Option<String> {
        let Some((segment, rest)) = segments.split_first() else {
            let len = match value {
                Value::Array(items) => items.len(),
                Value::Object(items) => items.len(),
                _ => 0,
            };
            return (len > 1).then(|| pointer.to_owned());
        };
        if *segment == "*" {
            return match value {
                Value::Object(map) => map.iter().find_map(|(key, child)| {
                    visit(child, rest, &format!("{pointer}/{}", escape_pointer(key)))
                }),
                Value::Array(items) if items.len() == 1 => {
                    visit(&items[0], rest, &format!("{pointer}/0"))
                }
                _ => None,
            };
        }
        let key = segment.replace("~1", "/").replace("~0", "~");
        let child = value.as_object()?.get(&key)?;
        visit(child, rest, &format!("{pointer}/{}", escape_pointer(&key)))
    }
    paths.iter().find_map(|path| {
        let segments = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        visit(row, &segments, "")
    })
}

/// A later slice of a split row repeats its skeleton; the lists of records
/// beside the paged array, at each level of its `pointer` (diagnostics
/// beside the paged files, a cycle's edges beside its files), were delivered
/// with the first part, so later slices carry them emptied: each record
/// once.
pub(super) fn empty_secondary_lists(skeleton: &mut Value, pointer: &str) {
    fn empty_records(value: &mut Value) {
        match value {
            Value::Array(items) if items.iter().all(|item| item.is_object() || item.is_array()) => {
                items.clear();
            }
            Value::Object(map) => {
                for (name, child) in map {
                    if name != PAGES_KEY && name != HINTS_KEY {
                        empty_records(child);
                    }
                }
            }
            _ => {}
        }
    }
    let mut segments = pointer.trim_start_matches('/').split('/');
    if segments.next() != Some("data") {
        return;
    }
    let mut node = skeleton.get_mut("data");
    for segment in segments {
        let key = segment.replace("~1", "/").replace("~0", "~");
        let Some(current) = node else {
            return;
        };
        if let Some(map) = current.as_object_mut() {
            for (name, value) in map.iter_mut() {
                if *name == key || name == PAGES_KEY || name == HINTS_KEY {
                    continue;
                }
                // Records (objects, or tuples such as a runtime cycle's
                // members), including nested lists, not scalar descriptors
                // or executable continuation queries.
                empty_records(value);
            }
        }
        node = match current {
            Value::Object(map) => map.get_mut(&key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|at| items.get_mut(at)),
            _ => None,
        };
    }
}

/// A later slice of a split element (a pull request whose comments page)
/// names the element, not its description again: on each array element the
/// paged `pointer` passes through, its descriptive scalars (a body, a head
/// SHA) ride the first part only; the fields its entry schema requires
/// (`required`), its lists (emptied beside the paged one, and dropped by
/// [`drop_emptied_lists`] once the part is whole) and nested objects stay.
pub(super) fn lean_elements(
    skeleton: &mut Value,
    pointer: &str,
    required: &std::collections::BTreeSet<String>,
) {
    let segments = pointer
        .trim_start_matches('/')
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let mut node = Some(skeleton);
    let mut in_element = false;
    for key in &segments {
        let Some(current) = node else {
            return;
        };
        if in_element && let Some(map) = current.as_object_mut() {
            map.retain(|name, value| {
                name == key
                    || name == PAGES_KEY
                    || name == HINTS_KEY
                    || required.contains(name)
                    || value.is_array()
                    || value.is_object()
            });
        }
        in_element = current.is_array();
        node = match current {
            Value::Object(map) => map.get_mut(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
    }
}

/// On a later slice, each array element the paged `pointer` passes through
/// drops its lists left empty (an emptied `reviews: []` says nothing) unless
/// its entry schema requires them; the paged array itself stays.
pub(super) fn drop_emptied_lists(
    value: &mut Value,
    pointer: &str,
    required: &std::collections::BTreeSet<String>,
) {
    let segments = pointer
        .trim_start_matches('/')
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let mut node = Some(value);
    let mut in_element = false;
    for key in &segments {
        let Some(current) = node else {
            return;
        };
        if in_element && let Some(map) = current.as_object_mut() {
            map.retain(|name, value| {
                name == key
                    || required.contains(name)
                    || value.as_array().is_none_or(|items| !items.is_empty())
            });
        }
        in_element = current.is_array();
        node = match current {
            Value::Object(map) => map.get_mut(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
    }
}
