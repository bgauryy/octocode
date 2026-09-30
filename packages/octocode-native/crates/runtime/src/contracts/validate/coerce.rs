//! Lossless string-to-scalar coercion for typed query fields.
//!
//! Agents and shells often send `startLine:"10"` or `fullContent:"true"`. A
//! string is coerced only when every schema that can hold the field types it
//! `integer` or `boolean`; a field any branch leaves untyped or accepts as a
//! string is untouched. Integer: the string is exactly the decimal rendering
//! of a safe integer (JS: `Number.isSafeInteger(n) && String(n) === s`), so
//! `"02"`, `"+2"`, `"-0"`, `"2.0"`, `"1e3"` and padded values stay strings.
//! Boolean: exactly `"true"` or `"false"`.

use serde_json::Value;

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const MAX_REF_DEPTH: usize = 32;

/// A schema and the root its `$ref`s resolve against.
type Typed<'a> = (&'a Value, &'a Value);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Integer,
    Boolean,
}

pub(super) fn coerce_scalar_strings(candidates: &[Typed<'_>], value: &mut Value) {
    let mut schemas = Vec::new();
    for &(root, schema) in candidates {
        flatten(root, schema, 0, &mut schemas);
    }
    if schemas.is_empty() {
        return;
    }
    match value {
        Value::String(text) => {
            if let Some(coerced) = agreed_scalar(&schemas).and_then(|kind| parse(kind, text)) {
                *value = coerced;
            }
        }
        Value::Object(object) => {
            for (key, field) in object.iter_mut() {
                let children = schemas
                    .iter()
                    .filter_map(|&(root, schema)| {
                        schema
                            .get("properties")?
                            .get(key)
                            .map(|child| (root, child))
                    })
                    .collect::<Vec<_>>();
                coerce_scalar_strings(&children, field);
            }
        }
        Value::Array(items) => {
            let children = schemas
                .iter()
                .filter_map(|&(root, schema)| schema.get("items").map(|child| (root, child)))
                .collect::<Vec<_>>();
            for item in items {
                coerce_scalar_strings(&children, item);
            }
        }
        _ => {}
    }
}

/// Resolve `$ref`s and expand union branches into the concrete schemas a
/// value may be checked against. `{"not":{}}` forbids the field in that
/// branch, so it cannot hold the value and is dropped.
fn flatten<'a>(root: &'a Value, schema: &'a Value, depth: usize, out: &mut Vec<Typed<'a>>) {
    if depth > MAX_REF_DEPTH {
        out.push((root, &Value::Null));
        return;
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = root
            .pointer(reference.strip_prefix('#').unwrap_or(reference))
            .unwrap_or(&Value::Null);
        return flatten(root, target, depth + 1, out);
    }
    let branches = ["oneOf", "anyOf", "allOf"]
        .iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .collect::<Vec<_>>();
    if !branches.is_empty() {
        for branch in branches {
            flatten(root, branch, depth + 1, out);
        }
        return;
    }
    let forbidden = schema.as_object().is_some_and(|object| {
        object.len() == 1
            && object
                .get("not")
                .is_some_and(|not| not == &Value::Object(Default::default()))
    });
    if !forbidden {
        out.push((root, schema));
    }
}

/// The one scalar type every alternative agrees on; a `null` alternative is
/// neutral, any other type (string, untyped) vetoes coercion.
fn agreed_scalar(schemas: &[Typed<'_>]) -> Option<Scalar> {
    let mut agreed = None;
    for (_, schema) in schemas {
        let types = match schema.get("type") {
            Some(Value::String(name)) => vec![name.as_str()],
            Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
            _ => return None,
        };
        for name in types {
            let kind = match name {
                "null" => continue,
                "integer" => Scalar::Integer,
                "boolean" => Scalar::Boolean,
                _ => return None,
            };
            if agreed
                .replace(kind)
                .is_some_and(|previous| previous != kind)
            {
                return None;
            }
        }
    }
    agreed
}

fn parse(kind: Scalar, text: &str) -> Option<Value> {
    match kind {
        Scalar::Integer => {
            let number = text.parse::<i64>().ok()?;
            (number.unsigned_abs() <= MAX_SAFE_INTEGER && number.to_string() == text)
                .then(|| Value::from(number))
        }
        Scalar::Boolean => match text {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::coerce_scalar_strings;
    use serde_json::{Value, json};

    fn coerced(schema: &Value, value: Value) -> Value {
        let mut value = json!({"field": value});
        let object = json!({"type":"object","properties":{"field":schema}});
        coerce_scalar_strings(&[(&object, &object)], &mut value);
        value["field"].take()
    }

    #[test]
    fn nullable_alternatives_coerce_and_string_alternatives_veto() {
        let nullable = json!({"anyOf":[{"type":"integer"},{"type":"null"}]});
        assert_eq!(coerced(&nullable, json!("3")), json!(3));
        assert_eq!(
            coerced(&json!({"type":["boolean","null"]}), json!("true")),
            json!(true)
        );
        let widened = json!({"anyOf":[{"type":"integer"},{"type":"string"}]});
        assert_eq!(coerced(&widened, json!("3")), json!("3"));
        assert_eq!(coerced(&json!({}), json!("3")), json!("3"));
        let mixed = json!({"anyOf":[{"type":"integer"},{"type":"boolean"}]});
        assert_eq!(coerced(&mixed, json!("true")), json!("true"));
    }

    #[test]
    fn only_canonical_decimal_safe_integers_coerce() {
        let integer = json!({"type":"integer"});
        for (text, expected) in [
            ("0", json!(0)),
            ("-7", json!(-7)),
            ("9007199254740991", json!(9_007_199_254_740_991_i64)),
        ] {
            assert_eq!(coerced(&integer, json!(text)), expected, "{text}");
        }
        for text in [
            " 3",
            "3.0",
            "1e2",
            "+3",
            "03",
            "-0",
            "0x10",
            "9007199254740992",
            "",
        ] {
            assert_eq!(coerced(&integer, json!(text)), json!(text), "{text}");
        }
    }
}
