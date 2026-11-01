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

/// A later slice of a split element (a pull request whose comments page)
/// names the element, not its description again: on each array element the
/// paged `pointer` passes through, descriptive scalars ride the first part
/// only. Required fields and core-classified provenance/disclosures stay,
/// along with its lists (emptied beside the paged one, and dropped by
/// the final materialization pass once the part is whole) and nested objects stay.
pub(super) fn lean_elements(
    skeleton: &mut Value,
    pointer: &str,
    context: &crate::contracts::output_fields::PageContextFields,
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
                    || context.required.contains(name)
                    || context.optional.contains(name)
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
