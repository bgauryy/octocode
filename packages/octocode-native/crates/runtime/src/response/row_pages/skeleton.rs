use super::collections::collection_paths;
use crate::response::result::{HINTS_KEY, PAGES_KEY};
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

pub(super) fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
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

/// A later part of a split row (a listing whose files page, a pull request
/// whose comments page) names the row and the elements it continues, not
/// their description again: on every object the paged `pointer` passes
/// through, the row itself included, a member rides the first part only
/// unless it is the pointer's next step, guidance, a core-declared
/// page-context field (`pageContextFields`: identity, provenance and
/// disclosures), or a holder of a declared collection (emptied beside the
/// paged one, and dropped by the final materialization pass once the part
/// is whole).
pub(super) fn lean_later_context(
    skeleton: &mut Value,
    pointer: &str,
    context: &crate::contracts::output_fields::PageContextFields,
    routes: &crate::contracts::output_fields::CollectionRoutes,
) {
    let holders = collection_paths(skeleton, routes);
    let holds = |path: &str| {
        holders.iter().any(|held| {
            held.strip_prefix(path)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with('/'))
        })
    };
    let mut node = Some(skeleton);
    let mut at = String::new();
    for segment in pointer.trim_start_matches('/').split('/') {
        let Some(current) = node else {
            return;
        };
        let key = segment.replace("~1", "/").replace("~0", "~");
        if let Some(map) = current.as_object_mut() {
            map.retain(|name, _| {
                *name == key
                    || name == PAGES_KEY
                    || name == HINTS_KEY
                    || context.required.contains(name)
                    || context.optional.contains(name)
                    || holds(&format!("{at}/{}", escape_pointer(name)))
            });
        }
        node = match current {
            Value::Object(map) => map.get_mut(&key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        };
        at = format!("{at}/{segment}");
    }
}
