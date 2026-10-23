//! The opt-in output stage. A field core classes `extra` belongs to an
//! `extras` kind (stats, receipts, echoes, diagnostics, secondary details);
//! it is not evidence, not next-call input, and not a disclosure. Default
//! rows drop it; a row whose `extras` names its kind keeps it. The paths are
//! the contract's `extraPaths` ([`ToolId::extra_paths`]), so emitters build
//! these fields unconditionally and branch on a kind only for real work.
use crate::id::ToolId;
use octocode_contract_types::ExtraKind;
use serde_json::{Map, Value};

/// Whether a raw query's `extras` names `kind`.
#[must_use]
pub(crate) fn wants(query: &Value, kind: ExtraKind) -> bool {
    query
        .get("extras")
        .and_then(Value::as_array)
        .is_some_and(|kinds| kinds.iter().any(|value| names(value, kind)))
}

/// Whether one `extras` entry is `kind`'s contract name.
pub(crate) fn names(value: &Value, kind: ExtraKind) -> bool {
    value
        .as_str()
        .and_then(|name| name.parse::<ExtraKind>().ok())
        == Some(kind)
}

/// [`wants`] for a contract `extraPaths` kind name.
fn wants_kind(query: &Value, kind: &str) -> bool {
    kind.parse::<ExtraKind>()
        .is_ok_and(|kind| wants(query, kind))
}

/// Every extra path of `tool`, whatever its kind.
#[cfg(test)]
pub(crate) fn all_paths(tool: ToolId) -> Vec<&'static str> {
    tool.extra_paths()
        .iter()
        .flat_map(|(_, paths)| paths.iter().copied())
        .collect()
}

/// Whether [`prune_row`] leaves the extra at `path` on a row of `query`:
/// its kind is wanted, its key is picked, the rows are sorted by it, or no
/// kind of `tool` lists it.
#[must_use]
pub fn keeps(tool: ToolId, query: &Value, path: &str) -> bool {
    tool.extra_paths()
        .iter()
        .filter(|(_, paths)| paths.contains(&path))
        .all(|(kind, _)| {
            wants_kind(query, kind) || picked(query, path) || sorted_by(tool, query, path)
        })
}

/// Drops `tool`'s extra fields from one result row unless its query names
/// their kind. Error rows are pruned too: an extra field explains the call,
/// not the failure.
/// A key a query's `fields` pick names is evidence the caller asked for,
/// and the field the query's `sort` orders rows by is what the order
/// shows (contract `sortKeyPaths`), so both stay whatever their kind.
pub(crate) fn prune_row(row: &mut Value, tool: ToolId, query: &Value) {
    for (kind, paths) in tool.extra_paths() {
        if wants_kind(query, kind) {
            continue;
        }
        for path in *paths {
            if picked(query, path) || sorted_by(tool, query, path) {
                continue;
            }
            if let Some(rest) = path.strip_prefix("results[].") {
                remove(row, &segments(rest));
            }
        }
    }
}

/// Whether the query's `sort` orders `tool`'s rows by the extra at `path`.
fn sorted_by(tool: ToolId, query: &Value, path: &str) -> bool {
    let Some(sort) = query.get("sort").and_then(Value::as_str) else {
        return false;
    };
    tool.sort_key_paths()
        .iter()
        .any(|(value, paths)| *value == sort && paths.contains(&path))
}

/// Whether `path` ends in a key the query's `fields` pick names.
fn picked(query: &Value, path: &str) -> bool {
    let key = path.rsplit('.').next().unwrap_or(path);
    query
        .get("fields")
        .and_then(Value::as_array)
        .is_some_and(|fields| fields.iter().any(|field| field.as_str() == Some(key)))
}

/// `a.b[].c` → `[("a", false), ("b", true), ("c", false)]`: each field name
/// and whether the path continues into every element of its array.
fn segments(path: &str) -> Vec<(&str, bool)> {
    path.split('.')
        .map(|segment| match segment.strip_suffix("[]") {
            Some(name) => (name, true),
            None => (segment, false),
        })
        .collect()
}

fn remove(value: &mut Value, path: &[(&str, bool)]) {
    let Some((&(name, each), rest)) = path.split_first() else {
        return;
    };
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if rest.is_empty() && !each {
        object.shift_remove(name);
        return;
    }
    match (each, object.get_mut(name)) {
        (true, Some(Value::Array(items))) => {
            for item in items {
                remove(item, rest);
            }
        }
        (false, Some(child)) => {
            let held = child.as_object().is_some_and(|fields| !fields.is_empty());
            remove(child, rest);
            // A block that held only verbose fields says nothing once they
            // go; the row's `data` itself always stays.
            if held && name != "data" && child.as_object().is_some_and(Map::is_empty) {
                object.shift_remove(name);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod sorted_tests;

#[cfg(test)]
mod picked_tests;
