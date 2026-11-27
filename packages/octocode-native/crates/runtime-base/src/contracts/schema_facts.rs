//! Defaults and bounds read from the embedded query schemas, so native code
//! never re-spells a contract default or maximum as a literal.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use serde_json::Value;

use crate::id::ToolId;
use octocode_engine::num::saturating_usize;

/// One `oneOf`/`anyOf` branch of a tool's query schema (or the whole schema).
struct Variant<'a> {
    /// `operation` values the branch admits; `None` when it has no discriminant.
    operations: Option<Vec<&'a str>>,
    /// Top-level properties, each with its `$ref` chain against `$defs`:
    /// the property schema first, the final definition last.
    fields: HashMap<&'a str, Vec<&'a Value>>,
    /// Top-level properties the branch requires: a query omitting one never
    /// takes this branch, so the branch applies none of its defaults.
    required: HashSet<&'a str>,
}

impl<'a> Variant<'a> {
    fn of(branch: &'a Value, defs: &'a Value) -> Self {
        let branch = resolve_ref(branch, defs);
        let fields = branch["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(field, schema)| (field.as_str(), ref_chain(schema, defs)))
            .collect::<HashMap<_, _>>();
        let operations = fields
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
        let required = branch["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        Self {
            operations,
            fields,
            required,
        }
    }
}

/// The schema a local `$ref` (`#/$defs/<name>`) names, followed through
/// chained references, else `schema` itself. A result that still carries
/// `$ref` names no definition in `defs`. The one `$ref` resolver for
/// contract schemas (every contract reference is a `$defs` entry).
pub(crate) fn resolve_ref<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    ref_chain(schema, defs).last().copied().unwrap_or(schema)
}

/// Read a keyword without losing annotations on intermediate references.
pub fn schema_keyword<'a>(schema: &'a Value, defs: &'a Value, keyword: &str) -> Option<&'a Value> {
    ref_chain(schema, defs)
        .into_iter()
        .find_map(|part| part.get(keyword))
}

/// `schema` and every definition its `$ref` chain reaches, in order. A
/// keyword declared beside a `$ref` (a `default` on a shared bounded type)
/// stays readable on the schema that declares it.
fn ref_chain<'a>(mut schema: &'a Value, defs: &'a Value) -> Vec<&'a Value> {
    let mut chain = vec![schema];
    let mut seen = HashSet::from([std::ptr::from_ref(schema)]);
    while let Some(target) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| {
            defs.get(name).or_else(|| {
                super::parsed_contract()
                    .ok()
                    .and_then(|contract| contract.get("$defs"))
                    .and_then(|shared| shared.get(name))
            })
        })
    {
        if !seen.insert(std::ptr::from_ref(target)) {
            break;
        }
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

/// Schemas for an object value at a key no `properties` names: typed record
/// entries, as [`child_schemas`] finds them for such a key.
pub(crate) fn additional_schemas<'a>(schema: &'a Value, defs: &'a Value) -> Vec<&'a Value> {
    members(schema, defs, |schema| {
        schema
            .get("additionalProperties")
            .filter(|schema| schema.is_object())
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

/// Object field names in declaration order: a schema's own properties, then
/// each composition branch's that no earlier one named. The order a value of
/// this schema serializes its keys in.
pub(crate) fn property_order<'a>(schema: &'a Value, defs: &'a Value) -> Vec<&'a str> {
    let mut order = Vec::new();
    for properties in members(schema, defs, |schema| schema.get("properties"))
        .into_iter()
        .filter_map(Value::as_object)
    {
        for name in properties.keys() {
            if !order.contains(&name.as_str()) {
                order.push(name.as_str());
            }
        }
    }
    order
}

/// The members an object schema requires whatever form its value takes: its
/// own `required` and each `allOf` part's, with the ones every `oneOf` or
/// `anyOf` branch requires (structureSearch `sort` requires `by` in each of
/// its three forms), through `$ref`s.
pub(crate) fn always_required<'a>(schema: &'a Value, defs: &'a Value) -> Vec<&'a str> {
    fn walk<'a>(schema: &'a Value, defs: &'a Value, open: &mut Vec<*const Value>) -> Vec<&'a str> {
        let schema = resolve_ref(schema, defs);
        let at = std::ptr::from_ref(schema);
        if open.contains(&at) {
            return Vec::new();
        }
        open.push(at);
        let mut required: Vec<&str> = schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut add = |names: Vec<&'a str>| {
            for name in names {
                if !required.contains(&name) {
                    required.push(name);
                }
            }
        };
        for part in schema["allOf"].as_array().into_iter().flatten() {
            add(walk(part, defs, open));
        }
        for key in ["oneOf", "anyOf"] {
            let Some((first, rest)) = schema[key].as_array().and_then(|all| all.split_first())
            else {
                continue;
            };
            let others: Vec<Vec<&str>> =
                rest.iter().map(|branch| walk(branch, defs, open)).collect();
            add(walk(first, defs, open)
                .into_iter()
                .filter(|name| others.iter().all(|other| other.contains(name)))
                .collect());
        }
        open.pop();
        required
    }
    walk(schema, defs, &mut Vec::new())
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
fn variants(tool: ToolId) -> Option<&'static [Variant<'static>]> {
    static VARIANTS: [OnceLock<Vec<Variant<'static>>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    let index = ToolId::ALL.iter().position(|id| *id == tool)?;
    let variants = VARIANTS[index].get_or_init(|| {
        let Ok(tool) = super::tool_contract(tool) else {
            return Vec::new();
        };
        let schema = resolve_ref(&tool["querySchema"], &Value::Null);
        let defs = &schema["$defs"];
        let branches = ["oneOf", "anyOf"]
            .iter()
            .find_map(|key| schema.get(*key).and_then(Value::as_array))
            .map_or_else(|| vec![schema], |branches| branches.iter().collect());
        branches
            .into_iter()
            .map(|branch| Variant::of(branch, defs))
            .collect()
    });
    (!variants.is_empty()).then_some(variants.as_slice())
}

/// `keyword` (`"default"`, `"maximum"`, …) that `tool`'s query schema declares
/// for the top-level `field`, over the variants admitting `operation` (every
/// variant when `None`). Along a `$ref` chain the outermost declaration wins. Each such variant that has `field` must declare the
/// same value; a missing or disagreeing declaration yields `None`. A
/// `default` applies only where the field may be omitted, so a variant
/// requiring the field has no say in it.
#[must_use]
pub fn query_schema_value(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    keyword: &str,
) -> Option<&'static Value> {
    declared_value(variants(tool)?, operation, field, keyword)
}

fn declared_value<'a>(
    variants: &[Variant<'a>],
    operation: Option<&str>,
    field: &str,
    keyword: &str,
) -> Option<&'a Value> {
    let mut found = None;
    for variant in variants {
        if let (Some(operation), Some(admitted)) = (operation, &variant.operations)
            && !admitted.contains(&operation)
        {
            continue;
        }
        if keyword == "default" && variant.required.contains(field) {
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

/// Whether any form of `tool`'s query rows declares the top-level field,
/// through every reference and nested union.
pub fn query_declares(tool: ToolId, field: &str) -> bool {
    static DECLARED: [OnceLock<std::collections::BTreeSet<&'static str>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    let Some(index) = ToolId::ALL.iter().position(|id| *id == tool) else {
        return false;
    };
    DECLARED[index]
        .get_or_init(|| {
            super::tool_contract(tool)
                .map(|tool| property_names(&tool["querySchema"], &tool["querySchema"]["$defs"]))
                .unwrap_or_default()
        })
        .contains(field)
}

#[must_use]
pub fn query_schema_number(
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
pub fn query_schema_max(tool: ToolId, operation: Option<&str>, field: &str) -> Option<usize> {
    query_schema_number(tool, operation, field, "maximum").map(saturating_usize)
}

#[cfg(test)]
mod tests;
