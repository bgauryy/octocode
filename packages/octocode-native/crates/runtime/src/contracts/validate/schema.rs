//! Generic JSON-schema validation.
//!
//! Tool-agnostic structural validation of a value against a (sub)schema:
//! `$ref`/`not`/`oneOf`/`anyOf` resolution, `const`/`enum`, and the
//! object/array/string/number/size checks. Union-branch selection lives in `super::union`.

pub(super) mod audit;
mod checks;
mod object;
mod pattern;
mod predicates;
mod relationships;
mod scalar;
use super::{ContractValidationError, issue, schema_issue, union};
use object::validate_object;
use pattern::{compiled_pattern, describe_unreadable_pattern, pattern_message};
use predicates::predicate_root;
pub(super) use predicates::probe;
use scalar::{check_size, validate_array, validate_number, validate_string};
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ValidationMode {
    Input,
    Output,
    /// Output validation of a schema that declares no defaults: nothing is
    /// ever written, so union branches and `not` probes check the value
    /// itself instead of a copy.
    ReadOnly,
}

pub(super) fn validate_schema(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    validate_with_mode(root, schema, value, path, ValidationMode::Output)
}

/// [`validate_schema`] for a schema with no `default` anywhere: `value` is
/// only read ([`ValidationMode::ReadOnly`]).
pub(super) fn validate_read_only(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    validate_with_mode(root, schema, value, path, ValidationMode::ReadOnly)
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
    match schema {
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => {
            return Err(issue("schema.false", path.clone(), "Value is forbidden"));
        }
        Value::Object(_) => {}
        _ => {
            return Err(super::internal(
                "Schema must be an object or boolean".into(),
            ));
        }
    }
    if mode == ValidationMode::Input {
        normalize_integer(schema, value);
    }
    reject_forbidden(root, schema, value, path, mode)?;
    if let Some(reference) = schema.get("$ref") {
        let target = reference
            .as_str()
            .filter(|reference| reference.starts_with("#/$defs/"))
            .and_then(|reference| root.pointer(&reference[1..]))
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
    let union_issues = union_issues(root, schema, value, path, mode)?;

    let mut intersection_issues = Vec::new();
    if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
        for branch in branches {
            if let Err(error) =
                validate_schema_with_ref_depth(root, branch, value, path, ref_depth, mode)
            {
                if error.is_contract_error() {
                    return Err(error);
                }
                intersection_issues.extend(error.issues);
            }
        }
    }

    check_literals(schema, value, path)?;
    let kind = declared_kind(schema, value, path)?;
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
    issues.extend(intersection_issues);
    if issues.is_empty() {
        relationships::validate(schema, value, path)?;
        checks::validate(schema, value, path)?;
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

/// Reject a value its schema's `not` accepts.
fn reject_forbidden(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &[String],
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    let Some(forbidden) = schema.get("not") else {
        return Ok(());
    };
    // Only acceptance matters here: probe speculatively, and an empty
    // schema accepts every value without a copy.
    let accepted = {
        let _speculation = union::Speculation::enter();
        if mode == ValidationMode::ReadOnly {
            probe(validate_with_mode(
                root,
                forbidden,
                value,
                &mut path.to_vec(),
                mode,
            ))?
        } else {
            let mut candidate = value.clone();
            probe(validate_with_mode(
                root,
                forbidden,
                &mut candidate,
                &mut path.to_vec(),
                mode,
            ))?
        }
    };
    if accepted {
        // `{"not":{}}` forbids the field outright in this variant.
        let message = if forbidden.as_object().is_some_and(serde_json::Map::is_empty) {
            "Field is not allowed in this variant"
        } else {
            "Value matches a forbidden schema"
        };
        return Err(issue("schema.not", path.to_vec(), message));
    }
    Ok(())
}

/// The issues of the value's `oneOf`/`anyOf` branches; a branch's pattern
/// too long to read is described by the note of the field they belong to.
fn union_issues(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &[String],
    mode: ValidationMode,
) -> Result<Vec<super::ValidationIssue>, ContractValidationError> {
    let mut union_issues = Vec::new();
    for (keyword, exclusive) in [("oneOf", true), ("anyOf", false)] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array)
            && let Err(error) = union::validate(root, branches, exclusive, value, path, mode)
        {
            if error.is_contract_error() {
                return Err(error);
            }
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
    Ok(union_issues)
}

/// Reject a value other than the schema's `const`, or outside its `enum`,
/// naming what is allowed.
fn check_literals(
    schema: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
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
            path.to_vec(),
            format!("Unexpected constant value; expected: {expected}"),
            schema,
            value,
        ));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.contains(value)
    {
        // Name the allowed values, and the one a near miss means, so a wrong
        // view/entryType/etc. is self-correcting.
        let allowed = values
            .iter()
            .map(|v| {
                v.as_str()
                    .map_or_else(|| v.to_string(), std::borrow::ToOwned::to_owned)
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(schema_issue(
            "schema.enum",
            path.to_vec(),
            format!("Value is outside the allowed enum; allowed: {allowed}"),
            schema,
            value,
        ));
    }
    Ok(())
}

/// The type the value is validated as: the schema's `type`, the declared
/// type the value has when it lists several, else the value's own.
fn declared_kind<'s>(
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

#[cfg(test)]
mod tests;
