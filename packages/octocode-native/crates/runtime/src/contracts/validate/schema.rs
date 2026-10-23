//! Generic JSON-schema validation.
//!
//! Tool-agnostic structural validation of a value against a (sub)schema:
//! `$ref`/`not`/`oneOf`/`anyOf` resolution, `const`/`enum`, and the
//! object/array/string/number/size checks. Per-tool rule logic lives in the
//! parent module; union-branch selection lives in `super::union`.

mod object;
mod pattern;
mod scalar;
use super::{ContractValidationError, issue, schema_issue, union};
use object::validate_object;
pub(super) use object::{fields_listing, fields_of_kind};
use pattern::{compiled_pattern, describe_unreadable_pattern, line_range_message, pattern_message};
pub(super) use scalar::{boolean_enum_hint, boolean_enum_value};
use scalar::{check_size, validate_array, validate_number, validate_string};
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ValidationMode {
    Input,
    Output,
}

pub(super) fn validate_schema(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    validate_with_mode(root, schema, value, path, ValidationMode::Output)
}

pub(super) fn validate_input_schema(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    validate_with_mode(root, schema, value, path, ValidationMode::Input)
}

pub(super) fn validate_with_mode(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    validate_schema_with_ref_depth(root, schema, value, path, 0, mode)
}

/// JSON integers include 1.0 and 1e0; generated integer types need an integer
/// representation. Only input positions declaring an integer normalize it.
fn normalize_integer(schema: &Value, value: &mut Value) {
    let integer = match schema.get("type") {
        Some(Value::String(kind)) => kind == "integer",
        Some(Value::Array(kinds)) => {
            kinds.iter().any(|kind| kind == "integer") && !kinds.iter().any(|kind| kind == "number")
        }
        _ => false,
    };
    if !integer || !value.is_f64() {
        return;
    }
    let Some(number) = value.as_f64().filter(|n| n.is_finite() && n.fract() == 0.0) else {
        return;
    };
    // The rounded u64::MAX is 2^64: keep the upper bound exclusive so the
    // cast cannot saturate. The negative i64 bound is exactly representable.
    if number >= 0.0 && number < u64::MAX as f64 {
        *value = Value::from(number as u64);
    } else if number >= i64::MIN as f64 && number < 0.0 {
        *value = Value::from(number as i64);
    }
}

fn validate_schema_with_ref_depth(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
    ref_depth: usize,
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    if mode == ValidationMode::Input {
        normalize_integer(schema, value);
    }
    if let Some(forbidden) = schema.get("not") {
        // Only acceptance matters here: probe speculatively, and an empty
        // schema accepts every value without a copy.
        let accepted = forbidden
            .as_object()
            .is_some_and(|schema| schema.is_empty())
            || {
                let _speculation = union::Speculation::enter();
                let mut candidate = value.clone();
                validate_with_mode(root, forbidden, &mut candidate, &mut path.clone(), mode).is_ok()
            };
        if accepted {
            // `{"not":{}}` forbids the field outright in this variant.
            let message = if forbidden
                .as_object()
                .is_some_and(|schema| schema.is_empty())
            {
                "Field is not allowed in this variant"
            } else {
                "Value matches a forbidden schema"
            };
            return Err(issue("schema.not", path.clone(), message));
        }
    }
    if let Some(reference) = schema.get("$ref") {
        let target = reference
            .as_str()
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .and_then(|name| root["$defs"].get(name))
            .filter(|_| ref_depth < 32)
            .ok_or_else(|| {
                issue(
                    "schema.unsupported-ref",
                    path.clone(),
                    format!("Unresolved schema reference: {reference}"),
                )
            })?;
        // Every reference in a chain can carry constraints of its own.
        validate_schema_with_ref_depth(root, target, value, path, ref_depth + 1, mode)?;
    }

    // Preserve successful branch defaults, but retain enclosing field errors
    // after a failed refinement so its discriminator remains diagnostic.
    let mut union_issues = Vec::new();
    for (keyword, exclusive) in [("oneOf", true), ("anyOf", false)] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array)
            && let Err(error) = union::validate(root, branches, exclusive, value, path, mode)
        {
            union_issues.extend(error.issues);
        }
    }
    // A branch's pattern too long to read falls back to the note of the
    // field whose branches they are (`path`: a string or a list of them).
    if let Some(note) = schema.get("description").and_then(Value::as_str) {
        for item in &mut union_issues {
            if item.path.starts_with(path)
                && item.path[path.len()..]
                    .iter()
                    .all(|segment| segment.parse::<usize>().is_ok())
            {
                describe_unreadable_pattern(item, note);
            }
        }
    }

    if let Some(constant) = schema.get("const")
        && value != constant
    {
        // Name the expected constant so a wrong variant selector is
        // self-correcting instead of a generic rejection.
        let expected = constant
            .as_str()
            .map_or_else(|| constant.to_string(), str::to_owned);
        return Err(schema_issue(
            "schema.const",
            path.clone(),
            format!("Unexpected constant value; expected: {expected}"),
            schema,
            value,
        ));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.contains(value)
    {
        // Name the allowed values, and the one a near miss means, so a wrong
        // resultView/entryType/etc. is self-correcting.
        let allowed = values
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), |s| s.to_owned()))
            .collect::<Vec<_>>()
            .join(", ");
        // A speculative probe reads only pass/fail: no hint.
        let hint = if union::speculative() {
            String::new()
        } else {
            super::enum_value::enum_hint(values, value, path)
        };
        return Err(schema_issue(
            "schema.enum",
            path.clone(),
            format!("Value is outside the allowed enum; allowed: {allowed}{hint}"),
            schema,
            value,
        ));
    }
    let kind = match schema.get("type") {
        Some(Value::String(kind)) => Some(kind.as_str()),
        Some(Value::Array(kinds)) if !kinds.is_empty() => {
            for kind in kinds {
                if !kind.as_str().is_some_and(known_type) {
                    return Err(issue(
                        "schema.unsupported-type",
                        path.clone(),
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
                    path.clone(),
                    format!("Expected one of the declared types: {}", schema["type"]),
                    schema,
                    value,
                )
            })?)
        }
        Some(other) => {
            return Err(issue(
                "schema.unsupported-type",
                path.clone(),
                format!("Unsupported schema type: {other}"),
            ));
        }
        // Keywords such as `required` and `minimum` also apply without `type`.
        None => Some(value_type(value)),
    };
    let own = match kind {
        Some("object") => validate_object(root, schema, value, path, mode),
        Some("array") => validate_array(root, schema, value, path, mode),
        Some("string") => validate_string(schema, value, path),
        Some("integer") => validate_number(schema, value, path, true),
        Some("number") => validate_number(schema, value, path, false),
        Some("boolean") if !value.is_boolean() => Err(schema_issue(
            "schema.type",
            path.clone(),
            match value.as_str() {
                Some(text @ ("true" | "false")) => {
                    format!("Expected boolean; send {text}, not \"{text}\"")
                }
                _ => "Expected boolean".to_owned(),
            },
            schema,
            value,
        )),
        Some("null") if !value.is_null() => Err(schema_issue(
            "schema.type",
            path.clone(),
            "Expected null",
            schema,
            value,
        )),
        Some("boolean" | "null") | None => Ok(()),
        Some(other) => Err(issue(
            "schema.unsupported-type",
            path.clone(),
            format!("Unsupported schema type: {other}"),
        )),
    };
    let mut issues = own.err().map_or_else(Vec::new, |error| error.issues);
    issues.extend(union_issues);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn known_type(kind: &str) -> bool {
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

/// The known field that takes line spans (`["95-105"]`: an array of
/// `start-end` strings), which a `start…`/`end…` pair of numbers spells.
pub(super) fn span_field(
    root: &Value,
    properties: &serde_json::Map<String, Value>,
) -> Option<String> {
    const SPAN: &str = "^[1-9]\\d*-[1-9]\\d*$";
    let defs = &root["$defs"];
    properties
        .iter()
        .find(|(_, schema)| {
            let items = &super::super::resolve_ref(schema, defs)["items"];
            super::super::resolve_ref(items, defs)["pattern"] == SPAN
        })
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests;
