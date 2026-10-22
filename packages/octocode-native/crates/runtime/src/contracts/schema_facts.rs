//! Defaults and bounds read from the embedded query schemas, so native code
//! never re-spells a contract default or maximum as a literal.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use serde_json::Value;

use crate::id::ToolId;

/// One `oneOf`/`anyOf` branch of a tool's query schema (or the whole schema).
struct Variant {
    /// `operation` values the branch admits; `None` when it has no discriminant.
    operations: Option<Vec<&'static str>>,
    /// Top-level properties, `$ref`s resolved against `$defs`.
    fields: HashMap<&'static str, &'static Value>,
}

/// The schema a local `$ref` (`#/$defs/<name>`) names, followed through
/// chained references, else `schema` itself. A result that still carries
/// `$ref` names no definition in `defs`. The one `$ref` resolver for
/// contract schemas (every contract reference is a `$defs` entry).
pub(crate) fn resolve_ref<'a>(mut schema: &'a Value, defs: &'a Value) -> &'a Value {
    // A cycle of pure references cannot describe a value; the bound stops it.
    for _ in 0..32 {
        let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .and_then(|name| defs.get(name))
        else {
            break;
        };
        schema = target;
    }
    schema
}

/// Find a property's declared schemas through local references and composition
/// branches. Return each whole property schema: callers retain its own required,
/// allOf and union semantics. A branch without the property contributes nothing.
pub(crate) fn property_schemas<'a>(
    schema: &'a Value,
    defs: &'a Value,
    name: &str,
) -> Vec<&'a Value> {
    members(schema, defs, |schema| schema["properties"].get(name))
}

/// Object field names through local references and composition branches.
pub(crate) fn property_names<'a>(
    schema: &'a Value,
    defs: &'a Value,
) -> std::collections::BTreeSet<&'a str> {
    members(schema, defs, |schema| schema.get("properties"))
        .into_iter()
        .filter_map(Value::as_object)
        .flat_map(|properties| properties.keys().map(String::as_str))
        .collect()
}

/// Item schemas through references and composition branches.
pub(crate) fn item_schemas<'a>(schema: &'a Value, defs: &'a Value) -> Vec<&'a Value> {
    members(schema, defs, |schema| schema.get("items"))
}

fn members<'a>(
    schema: &'a Value,
    defs: &'a Value,
    member: impl Fn(&'a Value) -> Option<&'a Value>,
) -> Vec<&'a Value> {
    let mut found = Vec::new();
    let mut pending = vec![schema];
    let mut seen = HashSet::new();
    while let Some(schema) = pending.pop() {
        let schema = resolve_ref(schema, defs);
        if !seen.insert(std::ptr::from_ref(schema)) {
            continue;
        }
        if let Some(property) = member(schema) {
            found.push(property);
        }
        for key in ["allOf", "oneOf", "anyOf"] {
            pending.extend(schema[key].as_array().into_iter().flatten().rev());
        }
    }
    found
}

/// Query-schema variants of one tool, built on first use from that tool's
/// contract entry only (see [`super::tool_contract`]).
fn variants(tool: ToolId) -> Option<&'static [Variant]> {
    static VARIANTS: [OnceLock<Vec<Variant>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    let index = ToolId::ALL.iter().position(|id| *id == tool)?;
    let variants = VARIANTS[index].get_or_init(|| {
        let Ok(tool) = super::tool_contract(tool) else {
            return Vec::new();
        };
        let schema = &tool["querySchema"];
        let defs = &schema["$defs"];
        let branches = ["oneOf", "anyOf"]
            .iter()
            .find_map(|key| schema.get(*key).and_then(Value::as_array))
            .map_or_else(|| vec![schema], |branches| branches.iter().collect());
        branches
            .into_iter()
            .map(|branch| {
                let branch = resolve_ref(branch, defs);
                let fields = branch["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(field, schema)| (field.as_str(), resolve_ref(schema, defs)))
                    .collect::<HashMap<_, _>>();
                let operations = fields.get("operation").map(|operation| {
                    operation["const"].as_str().map_or_else(
                        || {
                            operation["enum"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .collect()
                        },
                        |single| vec![single],
                    )
                });
                Variant { operations, fields }
            })
            .collect()
    });
    (!variants.is_empty()).then_some(variants.as_slice())
}

/// `keyword` (`"default"`, `"maximum"`, …) that `tool`'s query schema declares
/// for the top-level `field`, over the variants admitting `operation` (every
/// variant when `None`). Each such variant that has `field` must declare the
/// same value; a missing or disagreeing declaration yields `None`.
#[must_use]
pub(crate) fn query_schema_value(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    keyword: &str,
) -> Option<&'static Value> {
    let mut found = None;
    for variant in variants(tool)? {
        if let (Some(operation), Some(admitted)) = (operation, &variant.operations)
            && !admitted.contains(&operation)
        {
            continue;
        }
        let Some(schema) = variant.fields.get(field) else {
            continue;
        };
        let value = schema.get(keyword)?;
        match found {
            Some(previous) if previous != value => return None,
            _ => found = Some(value),
        }
    }
    found
}

/// Top-level query fields validation can restore when absent: those with a
/// schema `default` (at any depth, so the set over-approximates), each with
/// the one default its schemas declare, `None` when they declare several (an
/// operation's own). A present field outside it never validates back after
/// removal.
pub(crate) fn restorable_fields(
    tool: ToolId,
) -> &'static HashMap<&'static str, Option<&'static Value>> {
    static FIELDS: [OnceLock<HashMap<&'static str, Option<&'static Value>>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    FIELDS[tool.index()].get_or_init(|| {
        let mut fields = HashMap::new();
        if let Ok(contract) = super::tool_contract(tool) {
            for schema in [&contract["querySchema"], &contract["inputSchema"]] {
                collect_defaulted(schema, &schema["$defs"], &mut fields);
            }
        }
        fields
    })
}

/// A field's default may sit on its own schema or on the definition it
/// references (`{"$ref":"#/$defs/HI_PageSize"}`).
fn collect_defaulted(
    schema: &'static Value,
    defs: &'static Value,
    fields: &mut HashMap<&'static str, Option<&'static Value>>,
) {
    match schema {
        Value::Object(object) => {
            if let Some(Value::Object(properties)) = object.get("properties") {
                for (name, field) in properties {
                    let Some(default) = resolve_ref(field, defs).get("default") else {
                        continue;
                    };
                    fields
                        .entry(name.as_str())
                        .and_modify(|known| {
                            if *known != Some(default) {
                                *known = None;
                            }
                        })
                        .or_insert(Some(default));
                }
            }
            object
                .values()
                .for_each(|child| collect_defaulted(child, defs, fields));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_defaulted(child, defs, fields)),
        _ => {}
    }
}

/// Whether any query-schema variant of `tool` declares the top-level `field`.
pub(crate) fn query_declares(tool: ToolId, field: &str) -> bool {
    variants(tool).is_some_and(|variants| {
        variants
            .iter()
            .any(|variant| variant.fields.contains_key(field))
    })
}

/// [`query_schema_value`] as an unsigned integer.
#[must_use]
pub(crate) fn query_schema_number(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    keyword: &str,
) -> Option<u64> {
    query_schema_value(tool, operation, field, keyword)?.as_u64()
}

/// The schema `maximum` of `field` as a `usize` clamp bound. Every call site
/// names a field the contract bounds; an undeclared bound is a contract
/// defect (a renamed or removed field), so it fails loudly instead of
/// silently disabling the clamp.
#[must_use]
#[expect(
    clippy::panic,
    reason = "a missing bound is a contract defect caught by the contract tests, never a request-time failure"
)]
pub(crate) fn query_schema_max(tool: ToolId, operation: Option<&str>, field: &str) -> usize {
    query_schema_number(tool, operation, field, "maximum")
        .and_then(|maximum| usize::try_from(maximum).ok())
        .unwrap_or_else(|| panic!("contract declares no maximum for {tool} {operation:?} {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_navigation_preserves_schemas_and_stops_reference_cycles() {
        let root = serde_json::json!({
            "$defs": {
                "success": {"allOf":[
                    {"properties":{"results":{"type":"array","items":{"$ref":"#/$defs/row"}}}},
                    {"$ref":"#/$defs/success"}
                ]},
                "row": {"oneOf":[
                    {"properties":{"data":{"allOf":[{"required":["base"]},{"oneOf":[{"required":["left"]},{"required":["right"]}]}]}}},
                    {"properties":{"data":{"required":["error"]}}}
                ]}
            },
            "anyOf":[{"required":["error"]},{"$ref":"#/$defs/success"}]
        });
        let defs = &root["$defs"];
        let results = property_schemas(&root, defs, "results");
        assert_eq!(results.len(), 1);
        let data = property_schemas(&results[0]["items"], defs, "data");
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["allOf"][0]["required"], serde_json::json!(["base"]));
        assert_eq!(
            data[0]["allOf"][1]["oneOf"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(data[1]["required"], serde_json::json!(["error"]));
        assert!(property_schemas(&root, defs, "missing").is_empty());
    }

    #[test]
    fn reads_resolved_and_variant_scoped_facts() {
        // `$ref` properties resolve; every history-item variant agrees.
        assert_eq!(
            query_schema_number(ToolId::GhGetHistoryItem, None, "length", "maximum"),
            Some(100_000)
        );
        // Both structureSearch variants enforce the published maxDepth.
        assert_eq!(
            query_schema_number(ToolId::StructureSearch, None, "maxDepth", "maximum"),
            Some(20)
        );
        assert_eq!(
            query_schema_number(ToolId::StructureSearch, Some("tree"), "maxDepth", "maximum"),
            Some(20)
        );
        assert_eq!(
            query_schema_value(ToolId::GhSearchRepo, None, "sort", "default"),
            Some(&Value::from("best-match"))
        );
        assert_eq!(
            query_schema_value(ToolId::GhSearchRepo, None, "nope", "default"),
            None
        );
    }

    #[test]
    #[should_panic(expected = "contract declares no maximum")]
    fn an_undeclared_clamp_bound_fails_loudly() {
        let _ = query_schema_max(ToolId::GhSearchRepo, None, "nope");
    }

    /// Every schema fact a runtime call site reads (with a fallback for an
    /// undeclared value) must stay declared, so no fallback is ever live.
    #[test]
    fn every_fact_the_runtime_reads_is_declared() {
        let facts: &[(ToolId, Option<&str>, &str, &str)] = &[
            (ToolId::GhGetHistoryItem, None, "length", "maximum"),
            (
                ToolId::GhGetHistoryItem,
                Some("issue"),
                "pageSize",
                "default",
            ),
            (
                ToolId::GhGetHistoryItem,
                Some("pullRequest"),
                "pageSize",
                "maximum",
            ),
            (ToolId::GhSearchHistory, None, "pageSize", "default"),
            (ToolId::GhSearchHistory, None, "pageSize", "maximum"),
            (ToolId::GhSearchCode, None, "pageSize", "maximum"),
            (ToolId::GhSearchRepo, None, "pageSize", "maximum"),
            (ToolId::GhStructure, None, "pageSize", "maximum"),
            (ToolId::AstSearch, Some("match"), "matchPageSize", "maximum"),
            (
                ToolId::AstSearch,
                Some("match"),
                "matchContentLength",
                "default",
            ),
            (
                ToolId::AstSearch,
                Some("match"),
                "matchContentLength",
                "maximum",
            ),
            (ToolId::AstSearch, Some("match"), "pageSize", "maximum"),
            (ToolId::StructureSearch, None, "maxEntries", "maximum"),
            (ToolId::StructureSearch, None, "pageSize", "maximum"),
            (ToolId::Clasify, None, "mainGoal", "maxLength"),
        ];
        for (tool, operation, field, keyword) in facts {
            assert!(
                query_schema_value(*tool, *operation, field, keyword).is_some(),
                "{tool} {operation:?} {field}.{keyword} is not declared"
            );
        }
    }
}
