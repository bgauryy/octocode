//! A number written as an integral float (`3.0`) where the schema declares
//! an integer only: validation accepts it, so it is read as the integer and
//! said, like every other lenient reading.

use serde_json::{Number, Value};

use super::super::schema_facts::{child_schemas, item_schemas, resolve_ref};
use super::{Note, notes};

/// Each integral float of `value`, held where every schema at its place
/// declares `integer`, replaced by that integer with a note.
pub(super) fn read_integral_floats(
    query: &Value,
    row: usize,
    value: &mut Value,
    notes: &mut Vec<Note>,
) {
    let defs = &query["$defs"];
    let mut path = Vec::new();
    walk(&[query], defs, value, &mut path, &mut |path, slot| {
        // An integral float below 2^53 prints as its exact integer digits.
        let Some(whole) = slot
            .as_f64()
            .filter(|number| number.fract() == 0.0 && number.abs() < 9.0e15)
            .and_then(|number| number.to_string().parse::<i64>().ok())
        else {
            return;
        };
        let label = notes::label(path);
        let meant = Value::Number(Number::from(whole));
        notes.push((Some(row), format!("{label}: {meant}")));
        *slot = meant;
    });
}

fn declares_integer_only(schema: &Value, defs: &Value) -> bool {
    let schema = resolve_ref(schema, defs);
    match &schema["type"] {
        Value::String(kind) => kind == "integer",
        Value::Array(kinds) => {
            kinds.iter().any(|kind| kind == "integer") && !kinds.iter().any(|kind| kind == "number")
        }
        _ => false,
    }
}

fn walk(
    schemas: &[&Value],
    defs: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
    read: &mut dyn FnMut(&[String], &mut Value),
) {
    match value {
        Value::Number(number) if number.is_f64() => {
            if !schemas.is_empty()
                && schemas
                    .iter()
                    .all(|schema| declares_integer_only(schema, defs))
            {
                read(path, value);
            }
        }
        Value::Object(fields) => {
            for (name, field) in fields.iter_mut() {
                let below: Vec<&Value> = schemas
                    .iter()
                    .flat_map(|schema| child_schemas(schema, defs, name))
                    .collect();
                path.push(name.clone());
                walk(&below, defs, field, path, read);
                path.pop();
            }
        }
        Value::Array(items) => {
            let below: Vec<&Value> = schemas
                .iter()
                .flat_map(|schema| item_schemas(schema, defs))
                .collect();
            for (index, item) in items.iter_mut().enumerate() {
                path.push(index.to_string());
                walk(&below, defs, item, path, read);
                path.pop();
            }
        }
        _ => {}
    }
}
