//! Defaults and bounds read from the embedded query schemas, so native code
//! never re-spells a contract default or maximum as a literal.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use serde_json::Value;

use crate::tools::id::ToolId;

/// One `oneOf`/`anyOf` branch of a tool's query schema (or the whole schema).
struct Variant {
    /// `operation` values the branch admits; `None` when it has no discriminant.
    operations: Option<Vec<&'static str>>,
    /// Top-level properties, `$ref`s resolved against `$defs`.
    fields: HashMap<&'static str, &'static Value>,
}

/// The schema a local `$ref` (`#/$defs/<name>`) names, else `schema`
/// itself. The one `$ref` resolver for contract schemas.
pub(crate) fn resolve_ref<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| defs.get(name))
        .unwrap_or(schema)
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
pub fn query_schema_value(
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

/// Top-level query fields validation can restore when absent: observed
/// defaults and schema `default`s (at any depth, so the set over-approximates).
/// A present field outside it never validates back after removal.
pub(crate) fn restorable_fields(tool: ToolId) -> Option<&'static HashSet<&'static str>> {
    static FIELDS: [OnceLock<HashSet<&'static str>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    let index = ToolId::ALL.iter().position(|id| *id == tool)?;
    let fields = FIELDS[index].get_or_init(|| {
        let mut fields = HashSet::new();
        let Ok(contract) = super::tool_contract(tool) else {
            return fields;
        };
        for candidate in contract["defaults"].as_array().into_iter().flatten() {
            for path in candidate["values"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(path, _)| path)
            {
                fields.extend(path.split('.').next());
            }
        }
        for schema in [&contract["querySchema"], &contract["inputSchema"]] {
            collect_defaulted(schema, &mut fields);
        }
        fields
    });
    Some(fields)
}

fn collect_defaulted(schema: &'static Value, fields: &mut HashSet<&'static str>) {
    match schema {
        Value::Object(object) => {
            if let Some(Value::Object(properties)) = object.get("properties") {
                fields.extend(
                    properties
                        .iter()
                        .filter(|(_, field)| field.get("default").is_some())
                        .map(|(name, _)| name.as_str()),
                );
            }
            object
                .values()
                .for_each(|child| collect_defaulted(child, fields));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_defaulted(child, fields)),
        _ => {}
    }
}

/// [`query_schema_value`] as an unsigned integer.
#[must_use]
pub fn query_schema_number(
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
#[allow(clippy::panic)]
pub fn query_schema_max(tool: ToolId, operation: Option<&str>, field: &str) -> usize {
    query_schema_number(tool, operation, field, "maximum")
        .and_then(|maximum| usize::try_from(maximum).ok())
        .unwrap_or_else(|| panic!("contract declares no maximum for {tool} {operation:?} {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            (
                ToolId::GhGetHistoryItem,
                Some("pullRequest"),
                "minify",
                "default",
            ),
            (ToolId::GhSearchHistory, None, "pageSize", "default"),
            (ToolId::GhSearchHistory, None, "pageSize", "maximum"),
            (ToolId::GhSearchCode, None, "pageSize", "maximum"),
            (ToolId::GhSearchRepo, None, "pageSize", "maximum"),
            (ToolId::GhStructure, None, "pageSize", "maximum"),
            (ToolId::AstRewrite, None, "maxFiles", "default"),
            (ToolId::AstRewrite, None, "maxMatches", "default"),
            (ToolId::AstRewrite, None, "page", "default"),
            (ToolId::AstRewrite, None, "pageSize", "default"),
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
            (ToolId::AstTopology, None, "maxFiles", "maximum"),
            (ToolId::AstTopology, None, "pageSize", "maximum"),
            (ToolId::AstTopology, None, "diagnosticPageSize", "maximum"),
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
