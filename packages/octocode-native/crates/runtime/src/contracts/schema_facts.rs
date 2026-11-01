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
    /// Top-level properties, each with its `$ref` chain against `$defs`:
    /// the property schema first, the final definition last.
    fields: HashMap<&'static str, Vec<&'static Value>>,
}

/// The schema a local `$ref` (`#/$defs/<name>`) names, followed through
/// chained references, else `schema` itself. A result that still carries
/// `$ref` names no definition in `defs`. The one `$ref` resolver for
/// contract schemas (every contract reference is a `$defs` entry).
pub(crate) fn resolve_ref<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    ref_chain(schema, defs).last().copied().unwrap_or(schema)
}

/// `schema` and every definition its `$ref` chain reaches, in order. A
/// keyword declared beside a `$ref` (a `default` on a shared bounded type)
/// stays readable on the schema that declares it.
fn ref_chain<'a>(mut schema: &'a Value, defs: &'a Value) -> Vec<&'a Value> {
    let mut chain = vec![schema];
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
        chain.push(schema);
    }
    chain
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

/// Schemas for an object value at a named key, including typed record entries.
/// Unconstrained extra data contributes no traversal schema.
pub(crate) fn child_schemas<'a>(schema: &'a Value, defs: &'a Value, name: &str) -> Vec<&'a Value> {
    members(schema, defs, |schema| {
        schema["properties"].get(name).or_else(|| {
            schema
                .get("additionalProperties")
                .filter(|schema| schema.is_object())
        })
    })
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
                    .map(|(field, schema)| (field.as_str(), ref_chain(schema, defs)))
                    .collect::<HashMap<_, _>>();
                let operations =
                    fields
                        .get("operation")
                        .and_then(|chain| chain.last())
                        .map(|operation| {
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
/// variant when `None`). Along a `$ref` chain the outermost declaration wins. Each such variant that has `field` must declare the
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
        let Some(chain) = variant.fields.get(field) else {
            continue;
        };
        let value = chain.iter().find_map(|schema| schema.get(keyword))?;
        match found {
            Some(previous) if previous != value => return None,
            _ => found = Some(value),
        }
    }
    found
}

/// Whether any query-schema variant declares the top-level field.
pub(crate) fn query_declares(tool: ToolId, field: &str) -> bool {
    variants(tool).is_some_and(|variants| {
        variants
            .iter()
            .any(|variant| variant.fields.contains_key(field))
    })
}

#[must_use]
pub(crate) fn query_schema_number(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    keyword: &str,
) -> Option<u64> {
    query_schema_value(tool, operation, field, keyword)?.as_u64()
}

/// The `maximum` core declares for a query field; `None` when it declares
/// none, so the field has no upper bound. A missing bound never panics a
/// call (`tools::contract_bounds_tests` checks every read is declared).
#[must_use]
pub(crate) fn query_schema_max(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
) -> Option<usize> {
    query_schema_number(tool, operation, field, "maximum")
        .map(|maximum| usize::try_from(maximum).unwrap_or(usize::MAX))
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

    /// The graphQuery page bounds are the contract's: a `default` declared
    /// beside the `$ref` to the bounded page type is read, not lost.
    #[test]
    fn keywords_beside_a_ref_are_read_along_the_chain() {
        assert_eq!(
            query_schema_number(ToolId::GraphQuery, None, "limit", "default"),
            Some(50)
        );
        assert_eq!(
            query_schema_number(ToolId::GraphQuery, None, "limit", "minimum"),
            Some(1)
        );
    }

    /// A field core bounds by nothing reads as unbounded, never a panic.
    #[test]
    fn an_undeclared_bound_reads_as_none() {
        assert_eq!(query_schema_max(ToolId::GhSearchRepo, None, "nope"), None);
        assert_eq!(query_schema_max(ToolId::GraphQuery, None, "depth"), None);
        assert_eq!(query_schema_max(ToolId::GraphQuery, None, "limit"), None);
    }
}
