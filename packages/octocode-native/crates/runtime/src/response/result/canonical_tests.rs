//! [`super::canonical_row`] over every tool's contract: a row stating each
//! defaulted field at its default parses to the same query as its canonical
//! form, so every field it drops re-applies as the same value.
use crate::id::ToolId;
use serde_json::{Map, Value, json};

fn resolve<'a>(mut schema: &'a Value, defs: &'a Value) -> &'a Value {
    for _ in 0..64 {
        let Some(name) = schema["$ref"].as_str().and_then(|r| r.strip_prefix("#/$defs/")) else {
            break;
        };
        schema = &defs[name];
    }
    schema
}

/// A value `schema` admits: its const, first enum member, default, or the
/// smallest value of its type.
fn sample(schema: &Value, defs: &Value) -> Value {
    let schema = resolve(schema, defs);
    if let Some(value) = schema.get("const").or_else(|| schema["enum"].get(0)) {
        return value.clone();
    }
    if let Some(value) = schema.get("default") {
        return value.clone();
    }
    if let Some(first) = ["anyOf", "oneOf"].iter().find_map(|key| schema[*key].get(0)) {
        return sample(first, defs);
    }
    match schema["type"].as_str() {
        Some("string") => json!("a".repeat(schema["minLength"].as_u64().unwrap_or(1).max(1) as usize)),
        Some("integer" | "number") => json!(schema["minimum"].as_i64().unwrap_or(1).max(1)),
        Some("boolean") => json!(false),
        Some("array") => json!([sample(&schema["items"], defs)]),
        Some("object") => object(schema, defs, false),
        _ => json!("a"),
    }
}

/// The required properties of `schema`, and with `defaults` every optional
/// one that declares a default, stated at that default.
fn object(schema: &Value, defs: &Value, defaults: bool) -> Value {
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    let mut row = Map::new();
    for (field, property) in schema["properties"].as_object().into_iter().flatten() {
        let declared = resolve(property, defs);
        if required.contains(&json!(field)) {
            row.insert(field.clone(), sample(property, defs));
        } else if defaults && let Some(default) = property.get("default").or(declared.get("default")) {
            row.insert(field.clone(), default.clone());
        }
    }
    Value::Object(row)
}

#[test]
fn every_dropped_default_parses_back_to_the_same_query() {
    let mut compared = 0;
    for tool in ToolId::ALL {
        let Ok(contract) = crate::contracts::tool_contract(tool) else {
            continue;
        };
        let schema = &contract["querySchema"];
        let defs = &schema["$defs"];
        let schema = resolve(schema, defs);
        let variants = ["oneOf", "anyOf"]
            .iter()
            .find_map(|key| schema[*key].as_array())
            .map_or_else(|| vec![schema], |branches| branches.iter().collect());
        for variant in variants {
            let row = object(resolve(variant, defs), defs, true);
            let Ok(parsed) = crate::contracts::validate_query(tool.as_str(), row.clone()) else {
                continue;
            };
            let mut canonical = row.clone();
            super::canonical_row(tool, &mut canonical);
            let reparsed = crate::contracts::validate_query(tool.as_str(), canonical.clone())
                .unwrap_or_else(|error| panic!("{}: {canonical} no longer parses: {error:?}", tool.as_str()));
            assert_eq!(
                parsed, reparsed,
                "{}: dropping defaults from {row} changed the query",
                tool.as_str()
            );
            compared += 1;
        }
    }
    // Every tool's contract yields at least one row this sampler can state.
    assert!(compared >= ToolId::ALL.len(), "only {compared} variants compared");
}
