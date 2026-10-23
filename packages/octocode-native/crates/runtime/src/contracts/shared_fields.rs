//! Leaf fields a response hoists into `shared` when every row entry holds
//! the same value.
//!
//! `shared` holds two kinds of keys: entry keys (hoisted from every object
//! entry of every array under `data`) and [`ROW_FIELDS`] (stated once for
//! every row, such as the checked-out head). A field an entry schema of the
//! responding tool requires is never hoisted, so the returned JSON validates
//! as it is: the output contract checks exactly what a client receives.
use crate::id::ToolId;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// Row-level fields a response may state once in `shared`. They are never
/// hoisted from entries, so a shared key has exactly one origin.
pub(crate) const ROW_FIELDS: &[&str] = &["commitSha"];

fn can_share_field(tool: ToolId, key: &str, value: &Value) -> bool {
    const EXCLUDED: &[&str] = &[
        "path",
        // A row's file, like its path, says what the row is about: it stays
        // on every row so no row is read against a trailing `shared` map.
        "file",
        "dir",
        "uri",
        "absolutePath",
        "owner",
        "repo",
        "name",
        // A declaration's name is its lspSearch anchor: every symbols row
        // keeps it.
        "symbolName",
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
        // Per-file counts are the count views' evidence: equal counts on a
        // page (common on later pages) must not move into `shared`.
        "matchCount",
        "matchedLineCount",
    ];
    !EXCLUDED.contains(&key)
        && !ROW_FIELDS.contains(&key)
        && !required_entry_fields(tool).contains(key)
        && (value.is_number()
            || value.is_boolean()
            || value.as_str().is_some_and(|s| !s.is_empty()))
}

/// Every field some array-entry schema of `tool`'s output contract
/// requires: hoisting one would remove it from entries whose schema demands
/// it. Derived once per tool from that tool's contract entry.
pub(crate) fn required_entry_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static REQUIRED: PerTool = [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    per_tool(&REQUIRED, tool, |schema| {
        let mut required = BTreeSet::new();
        let mut walk = EntryWalk {
            defs: &schema["$defs"],
            seen: std::collections::HashSet::new(),
            required: &mut required,
        };
        walk.visit(schema, false);
        required
    })
}

fn required_row_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static REQUIRED: PerTool = [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    per_tool(&REQUIRED, tool, |schema| {
        let defs = &schema["$defs"];
        let mut required = BTreeSet::new();
        for array in super::property_schemas(schema, defs, tool.output().rows_key()) {
            for row in super::item_schemas(array, defs) {
                for data in super::property_schemas(row, defs, "data") {
                    let mut walk = EntryWalk {
                        defs,
                        seen: std::collections::HashSet::new(),
                        required: &mut required,
                    };
                    walk.visit(data, true);
                }
            }
        }
        required
    })
}

/// One field table per tool, indexed by [`ToolId::index`].
type PerTool = [std::sync::OnceLock<BTreeSet<String>>; ToolId::ALL.len()];

/// `tool`'s table in `tables`, built on first use by `build` from that
/// tool's output schema only (see [`super::tool_contract`]): a response
/// never parses the contracts of tools it does not answer for. A tool the
/// contract lacks has no fields.
fn per_tool(
    tables: &'static PerTool,
    tool: ToolId,
    build: impl FnOnce(&Value) -> BTreeSet<String>,
) -> &'static BTreeSet<String> {
    tables[tool.index()].get_or_init(|| {
        super::tool_contract(tool)
            .map(|contract| build(&contract["outputSchema"]))
            .unwrap_or_default()
    })
}

/// One output schema's walk: inside an array's `items` (through references
/// and union branches) it collects each object's `required` names. Each
/// schema node is visited once per context, so recursive definitions end.
struct EntryWalk<'a, 'b> {
    defs: &'a Value,
    seen: std::collections::HashSet<(*const Value, bool)>,
    required: &'b mut BTreeSet<String>,
}

impl<'a> EntryWalk<'a, '_> {
    fn visit(&mut self, schema: &'a Value, entry: bool) {
        let schema = super::resolve_ref(schema, self.defs);
        if !self.seen.insert((std::ptr::from_ref(schema), entry)) {
            return;
        }
        let Some(object) = schema.as_object() else {
            return;
        };
        if entry {
            self.required.extend(
                object
                    .get("required")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        for (key, value) in object {
            match key.as_str() {
                "items" => self.visit(value, true),
                "oneOf" | "anyOf" | "allOf" => {
                    for branch in value.as_array().into_iter().flatten() {
                        self.visit(branch, entry);
                    }
                }
                "properties" | "patternProperties" => {
                    for property in value.as_object().into_iter().flat_map(|map| map.values()) {
                        self.visit(property, false);
                    }
                }
                "additionalProperties" => self.visit(value, false),
                _ => {}
            }
        }
    }
}

fn shared_leaves_mut(rows: &mut [Value]) -> impl Iterator<Item = &mut Map<String, Value>> {
    rows.iter_mut()
        .filter_map(|row| row["data"].as_object_mut())
        .flat_map(|data| data.values_mut().filter_map(Value::as_array_mut))
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// Hoist leaf fields every row entry of `tool`'s response shares into one
/// `shared` map.
pub(crate) fn hoist(tool: ToolId, rows: &mut [Value]) -> Option<Map<String, Value>> {
    let mut shared = Map::new();
    if rows.len() > 1 {
        for &key in ROW_FIELDS {
            if required_row_fields(tool).contains(key) {
                continue;
            }
            let Some(value) = rows[0]["data"]
                .get(key)
                .filter(|value| value.as_str().is_some_and(|text| !text.is_empty()))
                .cloned()
            else {
                continue;
            };
            if rows.iter().all(|row| row["data"].get(key) == Some(&value)) {
                for row in rows.iter_mut() {
                    if let Some(data) = row["data"].as_object_mut() {
                        data.shift_remove(key);
                    }
                }
                shared.insert(key.into(), value);
            }
        }
    }
    fn entries(row: &Value) -> Vec<&Map<String, Value>> {
        row["data"]
            .as_object()
            .into_iter()
            .flat_map(|data| data.values().filter_map(Value::as_array))
            .flatten()
            .filter_map(Value::as_object)
            .collect()
    }
    // `shared` holds what every row has: a row with no entry (an error, an
    // empty answer) leaves every entry field on its rows.
    if rows.iter().any(|row| entries(row).is_empty()) {
        return (!shared.is_empty()).then_some(shared);
    }
    let leaves: Vec<&Map<String, Value>> = rows.iter().flat_map(entries).collect();
    if leaves.len() < 2 {
        return (!shared.is_empty()).then_some(shared);
    }
    let entry_shared: Map<String, Value> = leaves[0]
        .iter()
        .filter(|(key, value)| {
            can_share_field(tool, key, value)
                && leaves.iter().all(|leaf| leaf.get(*key) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    for leaf in shared_leaves_mut(rows) {
        for key in entry_shared.keys() {
            // `remove` swaps the last key into the gap; the entry keeps the
            // order the tool wrote.
            leaf.shift_remove(key);
        }
    }
    shared.extend(entry_shared);
    (!shared.is_empty()).then_some(shared)
}

#[cfg(test)]
mod tests;
