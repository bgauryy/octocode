//! Defaults and bounds read from the embedded query schemas, so native code
//! never re-spells a contract default or maximum as a literal.

use std::{collections::HashMap, sync::OnceLock};

use serde_json::{Map, Value};

use crate::tools::id::ToolId;

/// One `oneOf`/`anyOf` branch of a tool's query schema (or the whole schema).
struct Variant {
    /// `operation` values the branch admits; `None` when it has no discriminant.
    operations: Option<Vec<&'static str>>,
    /// Top-level properties, `$ref`s resolved against `$defs`.
    fields: HashMap<&'static str, &'static Value>,
}

fn resolve(value: &'static Value, defs: &'static Value) -> &'static Value {
    value
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| defs.get(name))
        .unwrap_or(value)
}

fn index() -> &'static HashMap<&'static str, Vec<Variant>> {
    static INDEX: OnceLock<HashMap<&'static str, Vec<Variant>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        let Ok(contract) = super::parsed_contract() else {
            return index;
        };
        for tool in contract["tools"].as_array().into_iter().flatten() {
            let Some(name) = tool["name"].as_str() else {
                continue;
            };
            let schema = &tool["querySchema"];
            let defs = &schema["$defs"];
            let branches = ["oneOf", "anyOf"]
                .iter()
                .find_map(|key| schema.get(*key).and_then(Value::as_array))
                .map_or_else(|| vec![schema], |branches| branches.iter().collect());
            let variants = branches
                .into_iter()
                .map(|branch| {
                    let branch = resolve(branch, defs);
                    let fields = branch["properties"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .map(|(field, schema)| (field.as_str(), resolve(schema, defs)))
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
                .collect();
            index.insert(name, variants);
        }
        index
    })
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
    for variant in index().get(tool.as_str())? {
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

/// The schema `maximum` of `field` as a `usize` clamp bound. Validation already
/// enforces it, so an undeclared bound leaves the value unclamped.
#[must_use]
pub fn query_schema_max(tool: ToolId, operation: Option<&str>, field: &str) -> usize {
    query_schema_number(tool, operation, field, "maximum")
        .and_then(|maximum| usize::try_from(maximum).ok())
        .unwrap_or(usize::MAX)
}

/// Insert the schema default of each `fields` entry `query` omits. Output
/// continuation schemas require defaulted fields (the response stage compacts
/// them away again), so hand-built continuations stamp them from the contract.
pub fn stamp_schema_defaults(
    tool: ToolId,
    operation: Option<&str>,
    query: &mut Map<String, Value>,
    fields: &[&str],
) {
    for field in fields {
        if query.contains_key(*field) {
            continue;
        }
        if let Some(default) = query_schema_value(tool, operation, field, "default") {
            query.insert((*field).to_owned(), default.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_resolved_and_variant_scoped_facts() {
        // `$ref` properties resolve; every history-item variant agrees.
        assert_eq!(
            query_schema_number(ToolId::GhGetHistoryItem, None, "charLength", "maximum"),
            Some(100_000)
        );
        // Variants disagree on structureSearch maxDepth unless scoped.
        assert_eq!(
            query_schema_number(ToolId::StructureSearch, None, "maxDepth", "maximum"),
            None
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
        assert_eq!(
            query_schema_max(ToolId::GhSearchRepo, None, "nope"),
            usize::MAX
        );
    }

    /// Every schema fact a runtime call site reads (with a fallback for an
    /// undeclared value) must stay declared, so no fallback is ever live.
    #[test]
    fn every_fact_the_runtime_reads_is_declared() {
        let facts: &[(ToolId, Option<&str>, &str, &str)] = &[
            (ToolId::GhGetHistoryItem, None, "charLength", "maximum"),
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
            (ToolId::GhStructure, None, "metadataPage", "maximum"),
            (ToolId::GhStructure, None, "pageSize", "maximum"),
            (ToolId::AstRewrite, None, "maxFiles", "default"),
            (ToolId::AstRewrite, None, "maxMatches", "default"),
            (ToolId::AstRewrite, None, "page", "default"),
            (ToolId::AstRewrite, None, "pageSize", "default"),
            (
                ToolId::AstSearch,
                Some("match"),
                "maxMatchesPerFile",
                "maximum",
            ),
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
            (ToolId::AstTopology, None, "limit", "maximum"),
            (ToolId::AstTopology, None, "pageSize", "maximum"),
            (ToolId::AstTopology, None, "diagnosticPageSize", "maximum"),
            (ToolId::StructureSearch, None, "limit", "maximum"),
            (ToolId::StructureSearch, None, "pageSize", "maximum"),
            (ToolId::Clasify, None, "goal", "maxLength"),
        ];
        for (tool, operation, field, keyword) in facts {
            assert!(
                query_schema_value(*tool, *operation, field, keyword).is_some(),
                "{tool} {operation:?} {field}.{keyword} is not declared"
            );
        }
        for (tool, fields) in [
            (ToolId::GhSearchCode, &["page", "pageSize", "match"][..]),
            (ToolId::GhSearchRepo, &["page", "pageSize", "sort"][..]),
            (ToolId::GhStructure, &["page", "pageSize", "debug"][..]),
        ] {
            for field in fields {
                assert!(
                    query_schema_value(tool, None, field, "default").is_some(),
                    "{tool} {field} default is not declared"
                );
            }
        }
    }

    #[test]
    fn stamps_only_absent_defaulted_fields() {
        let mut query = Map::new();
        query.insert("page".into(), Value::from(3));
        stamp_schema_defaults(
            ToolId::GhSearchCode,
            None,
            &mut query,
            &["page", "pageSize", "match", "owner"],
        );
        assert_eq!(query["page"], 3);
        assert_eq!(query["pageSize"], 30);
        assert_eq!(query["match"], "file");
        assert!(!query.contains_key("owner"));
    }
}
