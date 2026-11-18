//! Schema-declared type selection and JSON value kinds.
use super::{ContractValidationError, issue, schema_issue};
use serde_json::Value;

/// The type the value is validated as: the schema's `type`, the declared
/// type the value has when it lists several, else the value's own.
pub(super) fn declared_kind<'s>(
    schema: &'s Value,
    value: &Value,
    path: &[String],
) -> Result<Option<&'s str>, ContractValidationError> {
    Ok(match schema.get("type") {
        Some(Value::String(kind)) => Some(kind.as_str()),
        Some(Value::Array(kinds)) if !kinds.is_empty() => {
            for kind in kinds {
                if !kind.as_str().is_some_and(known_type) {
                    return Err(issue(
                        "schema.unsupported-type",
                        path.to_vec(),
                        format!("Unsupported schema type: {kind}"),
                    ));
                }
            }
            let selected = kinds.iter().filter_map(Value::as_str).find(|kind| {
                *kind == value_type(value)
                    || (*kind == "integer"
                        && value.as_f64().is_some_and(|number| number.fract() == 0.0))
            });
            Some(selected.ok_or_else(|| {
                schema_issue(
                    "schema.type",
                    path.to_vec(),
                    format!("Expected one of the declared types: {}", schema["type"]),
                    schema,
                    value,
                )
            })?)
        }
        Some(other) => {
            return Err(issue(
                "schema.unsupported-type",
                path.to_vec(),
                format!("Unsupported schema type: {other}"),
            ));
        }
        // Keywords such as `required` and `minimum` also apply without `type`.
        None => Some(value_type(value)),
    })
}

pub(super) fn known_type(kind: &str) -> bool {
    matches!(
        kind,
        "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
    )
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}
