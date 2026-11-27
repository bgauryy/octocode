//! Test-only guard: every key a tool emits is declared by its core output
//! schema. The published output schemas stay open (`additionalProperties`
//! on loose objects), so JSON Schema validation alone never flags a field
//! native adds without declaring it; this walk does, for every response a
//! test validates.
use serde_json::{Map, Value};

/// Subtrees whose shape another contract owns: continuation and lead
/// queries are tool inputs, validated against that tool's input schema.
const INPUT_SHAPED: [&str; 2] = ["next", "hints"];

/// Envelope paths of `output` keys that no branch of `schema` declares.
pub(crate) fn undeclared_paths(schema: &Value, output: &Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(schema, &[schema], output, "", 0, &mut found);
    found.sort();
    found.dedup();
    found
}

/// `schema` with its `$ref` resolved; an unresolved reference declares nothing.
fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let resolved = super::resolve_ref(schema, &root["$defs"]);
    if resolved.get("$ref").is_some() {
        &Value::Null
    } else {
        resolved
    }
}

/// The schemas a value may match: union branches flattened, refs resolved.
fn branches<'a>(root: &'a Value, schemas: &[&'a Value], depth: usize) -> Vec<&'a Value> {
    let mut out = Vec::new();
    for schema in schemas {
        let schema = resolve(root, schema);
        let mut union = false;
        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(items) = schema.get(key).and_then(Value::as_array) {
                union = true;
                if depth < 24 {
                    out.extend(branches(root, &items.iter().collect::<Vec<_>>(), depth + 1));
                }
            }
        }
        if !union || schema.get("properties").is_some() {
            out.push(schema);
        }
    }
    out
}

/// A schema that names no keys: any object key is its content.
fn free_form(schema: &Map<String, Value>) -> bool {
    !schema.contains_key("properties") && !schema.contains_key("patternProperties")
}

fn walk(
    root: &Value,
    schemas: &[&Value],
    value: &Value,
    path: &str,
    depth: usize,
    found: &mut Vec<String>,
) {
    if depth > 48 {
        return;
    }
    let candidates = branches(root, schemas, 0);
    match value {
        Value::Object(object) => {
            let objects: Vec<&Map<String, Value>> = candidates
                .iter()
                .filter_map(|schema| schema.as_object())
                .collect();
            if objects.is_empty() || objects.iter().any(|schema| free_form(schema)) {
                return;
            }
            for (key, child) in object {
                if INPUT_SHAPED.contains(&key.as_str()) {
                    continue;
                }
                let mut declared: Vec<&Value> = Vec::new();
                for schema in &objects {
                    if let Some(property) = schema.get("properties").and_then(|p| p.get(key)) {
                        declared.push(property);
                    } else if let Some(patterns) =
                        schema.get("patternProperties").and_then(Value::as_object)
                    {
                        declared.extend(patterns.values());
                    }
                }
                if declared.is_empty() {
                    // A typed catch-all (`.catchall(schema)`) declares every key.
                    declared.extend(objects.iter().filter_map(|schema| {
                        schema.get("additionalProperties").filter(|extra| {
                            extra.as_object().is_some_and(|extra| !extra.is_empty())
                        })
                    }));
                }
                let child_path = format!("{path}.{key}");
                if declared.is_empty() {
                    found.push(child_path);
                } else {
                    walk(root, &declared, child, &child_path, depth + 1, found);
                }
            }
        }
        Value::Array(items) => {
            let element: Vec<&Value> = candidates
                .iter()
                .filter_map(|schema| schema.get("items"))
                .collect();
            if element.is_empty() {
                return;
            }
            let child_path = format!("{path}[]");
            for item in items {
                walk(root, &element, item, &child_path, depth + 1, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
