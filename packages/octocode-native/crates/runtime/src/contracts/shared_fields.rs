//! Leaf fields a response hoists into `shared` when every row entry holds
//! the same value, and the canonical view that restores them before the
//! output contract validates a response.
use serde_json::{Map, Value};

fn can_share_field(key: &str, value: &Value) -> bool {
    const EXCLUDED: &[&str] = &[
        "path",
        "dir",
        "uri",
        "absolutePath",
        "owner",
        "repo",
        "name",
        "id",
        "type",
        "kind",
        "reason",
        "isPartial",
        "number",
        "title",
        "state",
        "author",
        "labels",
        "createdAt",
        "mergedAt",
        "commentsCount",
        "startLine",
        "endLine",
        "start",
        "end",
        "startColumn",
        "endColumn",
        "startByte",
        "endByte",
        "line",
        "column",
        "character",
        "parentId",
        "parent",
        "named",
        "exported",
        // Per-entry match-row accounting must stay on each entry: hoisting it
        // whenever the values happen to coincide (typical on page 1) makes the
        // row shape depend on the data, so identical queries drift between
        // pages and between CLI and MCP consumers.
        "totalMatchRows",
        "returnedMatchRows",
    ];
    !EXCLUDED.contains(&key)
        && (value.is_number()
            || value.is_boolean()
            || value.as_str().is_some_and(|s| !s.is_empty()))
}

fn shared_leaves_mut(rows: &mut [Value]) -> impl Iterator<Item = &mut Map<String, Value>> {
    rows.iter_mut()
        .filter_map(|row| row["data"].as_object_mut())
        .flat_map(|data| data.values_mut().filter_map(Value::as_array_mut))
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// Restore the canonical evidence view for validation without changing the
/// compact response returned to the caller. Explicit leaf values take priority.
pub(crate) fn restore(output: &mut Value) {
    let shared = match output.get("shared").and_then(Value::as_object) {
        Some(shared) => shared.clone(),
        None => return,
    };
    let Some(rows) = output.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for leaf in shared_leaves_mut(rows) {
        for (key, value) in &shared {
            if can_share_field(key, value) {
                leaf.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

/// Hoist leaf fields every row entry shares into one `shared` map.
pub(crate) fn hoist(rows: &mut [Value]) -> Option<Map<String, Value>> {
    let leaves: Vec<&Map<String, Value>> = rows
        .iter()
        .filter_map(|row| row["data"].as_object())
        .flat_map(|data| data.values().filter_map(Value::as_array))
        .flatten()
        .filter_map(Value::as_object)
        .collect();
    if leaves.len() < 2 {
        return None;
    }
    let shared: Map<String, Value> = leaves[0]
        .iter()
        .filter(|(key, value)| {
            can_share_field(key, value) && leaves.iter().all(|leaf| leaf.get(*key) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if shared.is_empty() {
        return None;
    }
    for leaf in shared_leaves_mut(rows) {
        for key in shared.keys() {
            leaf.remove(key);
        }
    }
    Some(shared)
}
