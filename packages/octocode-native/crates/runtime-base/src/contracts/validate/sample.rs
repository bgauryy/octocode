//! Values a schema accepts, built from the schema alone: the shape a
//! rejected field is shown in when the request's own value cannot stand.

use super::super::schema_facts::{
    child_schemas, item_schemas, property_order, property_schemas, resolve_ref, schema_keyword,
};
use serde_json::Value;

/// How deep a sample descends into nested objects and lists.
const DEPTH: usize = 8;

/// A visible placeholder for a required `field` the rejected row lacks, in
/// the shape of the example's `value`: the field's name in capitals
/// (`repo` → `REPO`, `matchString` → `MATCH_STRING`), never a value that
/// reads as a real name. `None` for a value that is not text.
pub(super) fn placeholder(field: &str, value: &Value) -> Option<Value> {
    let mut name = String::new();
    for (at, character) in field.chars().enumerate() {
        if at > 0 && character.is_ascii_uppercase() {
            name.push('_');
        }
        name.push(character.to_ascii_uppercase());
    }
    match value {
        Value::String(_) => Some(Value::String(name)),
        Value::Array(items) if items.first().is_some_and(Value::is_string) => {
            Some(Value::Array(vec![Value::String(name)]))
        }
        _ => None,
    }
}

/// The schemas that describe the value at `path` (object keys, list
/// indexes) under `schema`: one per form of the schema that declares it.
pub(super) fn schemas_at<'a>(
    schema: &'a Value,
    defs: &'a Value,
    path: &[String],
) -> Vec<&'a Value> {
    let mut found = vec![schema];
    for segment in path {
        found = found
            .into_iter()
            .flat_map(|schema| {
                if segment.parse::<usize>().is_ok() {
                    item_schemas(schema, defs)
                } else {
                    child_schemas(schema, defs, segment)
                }
            })
            .collect();
    }
    found
}

/// A value `schema` accepts, or `None` when the schema gives no basis for
/// one (a text pattern, an unknown format). A declared `default`, `const`
/// or the first `enum` member stands for the value; an object holds each
/// property it declares; text is the placeholder of `name`
/// ([`placeholder`]) and a date is the epoch.
pub(super) fn sample(schema: &Value, defs: &Value, name: &str) -> Option<Value> {
    sample_within(schema, defs, name, 0)
}

fn sample_within(schema: &Value, defs: &Value, name: &str, depth: usize) -> Option<Value> {
    if depth > DEPTH {
        return None;
    }
    if let Some(default) = schema_keyword(schema, defs, "default") {
        return Some(default.clone());
    }
    let shape = resolve_ref(schema, defs);
    if let Some(constant) = shape.get("const") {
        return Some(constant.clone());
    }
    if let Some(first) = shape["enum"].as_array().and_then(|members| members.first()) {
        return Some(first.clone());
    }
    for union in ["oneOf", "anyOf"] {
        if let Some(branches) = shape[union].as_array() {
            return branches
                .iter()
                .find_map(|branch| sample_within(branch, defs, name, depth + 1));
        }
    }
    let kind = match &shape["type"] {
        Value::String(kind) => Some(kind.as_str()),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .find(|kind| *kind != "null"),
        _ => shape.get("properties").map(|_| "object"),
    }?;
    match kind {
        "object" => sample_object(shape, defs, depth),
        "array" => {
            let item = item_schemas(shape, defs)
                .into_iter()
                .find_map(|item| sample_within(item, defs, name, depth + 1))?;
            let count = shape["minItems"].as_u64().map_or(1, |least| least.max(1));
            Some(Value::Array(
                (0..count).map(|_| item.clone()).collect::<Vec<_>>(),
            ))
        }
        "string" => sample_text(shape, name),
        "integer" | "number" => Some(
            shape
                .get("minimum")
                .cloned()
                .or_else(|| {
                    let above = shape["exclusiveMinimum"].as_i64()?;
                    Some(Value::from(above + 1))
                })
                .unwrap_or_else(|| Value::from(0)),
        ),
        "boolean" => Some(Value::Bool(false)),
        _ => None,
    }
}

/// Text a string schema accepts: the first of the field's placeholder, an
/// epoch date-time, an epoch date and a one-day duration that passes the
/// schema's `format` and `pattern`; `None` when none does.
fn sample_text(shape: &Value, name: &str) -> Option<Value> {
    let placeholder = placeholder(name, &Value::String(String::new()))?;
    let pattern = shape["pattern"].as_str();
    let format = shape["format"].as_str();
    [
        placeholder.as_str().unwrap_or_default(),
        "1970-01-01T00:00:00Z",
        "1970-01-01",
        "-P1D",
    ]
    .into_iter()
    .find(|text| {
        let by_format = match format {
            None => true,
            Some("date-time") => text.contains('T') && text.ends_with('Z'),
            Some("date") => text.len() == 10 && text.starts_with(char::is_numeric),
            Some(_) => false,
        };
        by_format && pattern.is_none_or(|pattern| super::schema::pattern_matches(pattern, text))
    })
    .map(|text| Value::String(text.to_owned()))
}

/// An object holding each property its schema declares that has a sample;
/// `None` when a required property has none, or nothing is held.
fn sample_object(shape: &Value, defs: &Value, depth: usize) -> Option<Value> {
    let required = shape["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let mut fields = serde_json::Map::new();
    for name in property_order(shape, defs) {
        let value = property_schemas(shape, defs, name)
            .into_iter()
            .find_map(|property| sample_within(property, defs, name, depth + 1));
        match value {
            Some(value) => {
                fields.insert(name.to_owned(), value);
            }
            None if required.contains(&name) => return None,
            None => {}
        }
    }
    (!fields.is_empty() || shape.get("minProperties").is_none()).then_some(Value::Object(fields))
}

/// The row's `own` value of the field its `faults` lie in (each a path from
/// the field, as the row holds it), with each fault shown in a shape the
/// query schema accepts: a wrong value is replaced by a sample of its
/// schema ([`sample`]), a key the schema does not declare is removed, a
/// required key the value lacks is set. One candidate per form of the schema
/// that declares a fault, the plainest first.
pub(super) fn sampled(tool_name: &str, own: &Value, faults: &[&[String]]) -> Vec<Value> {
    let Some(tool) = crate::id::ToolId::from_name(tool_name)
        .and_then(|id| crate::contracts::tool_contract(id).ok())
    else {
        return Vec::new();
    };
    let schema = resolve_ref(&tool["querySchema"], &Value::Null);
    let defs = &schema["$defs"];
    let samples = faults
        .iter()
        .map(|fault| {
            let name = fault.last().map_or("", String::as_str);
            schemas_at(schema, defs, fault)
                .into_iter()
                .filter_map(|schema| sample(schema, defs, name))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let forms = samples.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let mut candidates: Vec<Value> = Vec::new();
    for form in 0..forms {
        let mut value = own.clone();
        for (fault, samples) in faults.iter().zip(&samples) {
            let sample = samples.get(form).or(samples.first());
            if !show_fault(&mut value, &fault[1..], sample) {
                break;
            }
        }
        if !candidates.contains(&value) {
            candidates.push(value);
        }
    }
    candidates
}

/// Sets `sample` at `path` under `value`, or removes what is there when
/// there is no sample; `false` when the path leads nowhere a fault can be
/// shown (the fault stays).
fn show_fault(value: &mut Value, path: &[String], sample: Option<&Value>) -> bool {
    let Some((leaf, parents)) = path.split_last() else {
        return sample.is_some_and(|sample| {
            *value = sample.clone();
            true
        });
    };
    let mut at = &mut *value;
    for segment in parents {
        at = match at {
            Value::Object(fields) => match fields.get_mut(segment) {
                Some(next) => next,
                None => return false,
            },
            Value::Array(items) => match segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get_mut(index))
            {
                Some(next) => next,
                None => return false,
            },
            _ => return false,
        };
    }
    match (at, sample) {
        (Value::Object(fields), Some(sample)) => {
            fields.insert(leaf.clone(), sample.clone());
            true
        }
        (Value::Object(fields), None) => {
            fields.shift_remove(leaf);
            true
        }
        (Value::Array(items), Some(sample)) => leaf
            .parse::<usize>()
            .ok()
            .and_then(|index| items.get_mut(index))
            .map(|item| *item = sample.clone())
            .is_some(),
        _ => false,
    }
}
