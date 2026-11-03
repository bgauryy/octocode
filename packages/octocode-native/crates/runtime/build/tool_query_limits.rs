use super::super::{invalid, rust_ident, to_screaming_snake};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

/// `keyword` of one field schema as an unsigned integer: declared on the
/// field itself, else the one value its `anyOf`/`oneOf` members declare (a
/// `string | string[]` field's `maxItems` lives on the array member).
fn field_fact(schema: &Value, defs: &Value, keyword: &str) -> Option<u64> {
    let chain = reference_chain(schema, defs);
    if let Some(value) = chain.iter().find_map(|part| part.get(keyword)) {
        return value.as_u64();
    }
    let mut found = None;
    for member in chain
        .iter()
        .flat_map(|schema| {
            ["anyOf", "oneOf"]
                .iter()
                .filter_map(|key| schema.get(*key).and_then(Value::as_array))
        })
        .flatten()
    {
        let Some(value) = field_fact(member, defs, keyword) else {
            continue;
        };
        match found {
            Some(previous) if previous != value => return None,
            _ => found = Some(value),
        }
    }
    found
}

/// One `oneOf`/`anyOf` branch of a query schema: the `operation` values it
/// admits (`None` without a discriminant) and its top-level properties.
pub(super) struct QueryVariant<'a> {
    operations: Option<Vec<&'a str>>,
    pub(super) fields: &'a Map<String, Value>,
}

/// The `oneOf`/`anyOf` branches of a tool's query schema (the schema itself
/// when it has none) that declare properties.
pub(super) fn query_variants<'a>(schema: &'a Value, defs: &'a Value) -> Vec<QueryVariant<'a>> {
    let schema = resolve_schema_ref(schema, defs);
    let branches = ["oneOf", "anyOf"]
        .iter()
        .find_map(|key| schema.get(*key).and_then(Value::as_array))
        .map_or_else(|| vec![schema], |branches| branches.iter().collect());
    let mut variants = Vec::new();
    for branch in branches {
        let branch = resolve_schema_ref(branch, defs);
        let Some(fields) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let operations = fields.get("operation").map(|operation| {
            let operation = resolve_schema_ref(operation, defs);
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
        variants.push(QueryVariant { operations, fields });
    }
    variants
}

/// Integer `default`/`maximum`/`maxItems`/`maxLength` facts of one tool's
/// top-level query fields, keyed `FIELD_KEYWORD`, over the query-schema
/// variants `admits` selects. A fact is emitted only when every selected
/// variant that has the field declares the same value (the agreement rule of
/// `contracts::query_schema_value`).
fn query_facts(
    variants: &[QueryVariant<'_>],
    defs: &Value,
    admits: impl Fn(&Option<Vec<&str>>) -> bool,
) -> BTreeMap<String, u64> {
    const KEYWORDS: [&str; 4] = ["default", "maximum", "maxItems", "maxLength"];
    let mut facts: BTreeMap<String, Option<u64>> = BTreeMap::new();
    for variant in variants
        .iter()
        .filter(|variant| admits(&variant.operations))
    {
        for (field, schema) in variant.fields {
            for keyword in KEYWORDS {
                let value = field_fact(schema, defs, keyword);
                let name = format!(
                    "{}_{}",
                    to_screaming_snake(field),
                    to_screaming_snake(keyword)
                );
                facts
                    .entry(name)
                    .and_modify(|previous| {
                        if *previous != value {
                            *previous = None;
                        }
                    })
                    .or_insert(value);
            }
        }
    }
    facts
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
        .collect()
}

pub(super) fn resolve_schema_ref<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    reference_chain(schema, defs)
        .last()
        .copied()
        .unwrap_or(schema)
}

pub(super) fn reference_chain<'a>(mut schema: &'a Value, defs: &'a Value) -> Vec<&'a Value> {
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    while seen.insert(std::ptr::from_ref(schema)) {
        chain.push(schema);
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
    chain
}

/// `contracts::query_limits::<tool>`: every agreed integer query-schema fact
/// as a `usize` constant, plus `<tool>::<operation>` for the variants one
/// operation admits, so tools never re-spell a contract bound.
pub(in super::super) fn render_query_limits(contract: &Value) -> Result<String, Box<dyn Error>> {
    let tools = contract
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("tool contract has no tools[]"))?;
    let render = |facts: &BTreeMap<String, u64>, indent: &str| {
        facts
            .iter()
            .map(|(name, value)| format!("{indent}pub const {name}: usize = {value};\n"))
            .collect::<String>()
    };
    let mut output = String::from(
        "// @generated by crates/runtime/build.rs from packages/octocode-config/contract/tool-contract.json.\n",
    );
    for tool in tools {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        let schema = tool
            .get("querySchema")
            .ok_or_else(|| invalid(format!("{name} has no querySchema")))?;
        let defs = &contract["$defs"];
        let variants = query_variants(schema, defs);
        output.push_str(&format!(
            "pub mod {} {{\n{}",
            rust_ident(name),
            render(&query_facts(&variants, defs, |_| true), "    ")
        ));
        let operations = variants
            .iter()
            .filter_map(|variant| variant.operations.as_ref())
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        for operation in operations {
            let facts = query_facts(&variants, defs, |admitted| {
                admitted
                    .as_ref()
                    .is_none_or(|admitted| admitted.contains(&operation))
            });
            output.push_str(&format!(
                "    pub mod {} {{\n{}    }}\n",
                rust_ident(operation),
                render(&facts, "        ")
            ));
        }
        output.push_str("}\n");
    }
    Ok(output)
}
