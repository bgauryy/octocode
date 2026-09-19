//! Generic JSON-schema validation.
//!
//! Tool-agnostic structural validation of a value against a (sub)schema:
//! `$ref`/`not`/`oneOf`/`anyOf` resolution, `const`/`enum`, and the
//! object/array/string/number/size checks. Per-tool rule logic lives in the
//! parent module; union-branch selection lives in `super::union`.

use super::{ContractValidationError, ValidationIssue, issue, schema_issue, union};
use regex::Regex;
use serde_json::Value;
use url::Url;

pub(super) fn validate_schema(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    if let Some(forbidden) = schema.get("not") {
        let mut candidate = value.clone();
        if validate_schema(root, forbidden, &mut candidate, &mut path.clone()).is_ok() {
            return Err(issue(
                "schema.not",
                path.clone(),
                "Value matches a forbidden schema",
            ));
        }
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = root
            .pointer(reference.strip_prefix('#').unwrap_or(reference))
            .ok_or_else(|| {
                issue(
                    "schema.unsupported-ref",
                    path.clone(),
                    format!("Unresolved schema reference: {reference}"),
                )
            })?;
        return validate_schema(root, target, value, path);
    }
    let union = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .map(|branches| (branches, true))
        .or_else(|| {
            schema
                .get("anyOf")
                .and_then(Value::as_array)
                .map(|branches| (branches, false))
        });
    if let Some((branches, exclusive)) = union {
        return union::validate(root, branches, exclusive, value, path);
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
        // Name the allowed values so a wrong resultView/entryType/etc. is
        // self-correcting instead of a generic rejection (GOTCHAS 2026-09-19).
        let allowed = values
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), |s| s.to_owned()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(schema_issue(
            "schema.enum",
            path.clone(),
            format!("Value is outside the allowed enum; allowed: {allowed}"),
            schema,
            value,
        ));
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => validate_object(root, schema, value, path),
        Some("array") => validate_array(root, schema, value, path),
        Some("string") => validate_string(schema, value, path),
        Some("integer") => validate_number(schema, value, path, true),
        Some("number") => validate_number(schema, value, path, false),
        Some("boolean") if !value.is_boolean() => Err(schema_issue(
            "schema.type",
            path.clone(),
            "Expected boolean",
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
    }
}

fn validate_object(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    let received = value.clone();
    let object = value.as_object_mut().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.clone(),
            "Expected object",
            schema,
            &received,
        )
    })?;
    let properties = schema.get("properties").and_then(Value::as_object);
    let mut issues = Vec::new();
    if let Some(name_schema) = schema.get("propertyNames") {
        for key in object.keys() {
            let mut name = Value::String(key.clone());
            let mut name_path = path.clone();
            name_path.push(key.clone());
            if let Err(error) = validate_schema(root, name_schema, &mut name, &mut name_path) {
                issues.extend(error.issues);
            }
        }
    }
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        let known_fields: Vec<serde_json::Value> = properties
            .map(|p| p.keys().map(|k| serde_json::json!(k)).collect())
            .unwrap_or_default();
        for key in object.keys() {
            if !properties.is_some_and(|known| known.contains_key(key)) {
                let mut field_path = path.clone();
                field_path.push(key.clone());
                issues.push(ValidationIssue {
                    rule_id: "schema.unknown-field".into(),
                    path: field_path,
                    message: format!("Unknown field: {key}"),
                    // Embed known fields so format_input_error can suggest alternatives.
                    schema: Some(serde_json::json!({ "knownFields": known_fields })),
                    received: None,
                });
            }
        }
    }
    if let Some(additional) = schema
        .get("additionalProperties")
        .filter(|value| value.is_object())
    {
        for (key, field) in object.iter_mut() {
            if properties.is_some_and(|known| known.contains_key(key)) {
                continue;
            }
            path.push(key.clone());
            let result = validate_schema(root, additional, field, path);
            path.pop();
            if let Err(error) = result {
                issues.extend(error.issues);
            }
        }
    }
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(required) {
            let mut field_path = path.clone();
            field_path.push(required.to_owned());
            issues.extend(
                schema_issue(
                    "schema.required",
                    field_path,
                    format!("Missing required field: {required}"),
                    properties
                        .and_then(|items| items.get(required))
                        .unwrap_or(&Value::Null),
                    &Value::Null,
                )
                .issues,
            );
        }
    }
    if let Some(properties) = properties {
        for (name, field_schema) in properties {
            if !object.contains_key(name)
                && let Some(default) = field_schema.get("default")
            {
                object.insert(name.clone(), default.clone());
            }
            if let Some(field) = object.get_mut(name) {
                path.push(name.clone());
                let result = validate_schema(root, field_schema, field, path);
                path.pop();
                if let Err(error) = result {
                    issues.extend(error.issues);
                }
            }
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn validate_array(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
) -> Result<(), ContractValidationError> {
    let received = value.clone();
    let array = value.as_array_mut().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.clone(),
            "Expected array",
            schema,
            &received,
        )
    })?;
    let mut issues = Vec::new();
    if let Err(error) = check_size(schema, array.len(), path) {
        issues.extend(error.issues);
    }
    if let Some(items) = schema.get("items") {
        for (index, item) in array.iter_mut().enumerate() {
            path.push(index.to_string());
            let result = validate_schema(root, items, item, path);
            path.pop();
            if let Err(error) = result {
                issues.extend(error.issues);
            }
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

fn validate_string(
    schema: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let string = value.as_str().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.to_vec(),
            "Expected string",
            schema,
            value,
        )
    })?;
    check_size(schema, string.chars().count(), path)?;
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        let regex = Regex::new(pattern).map_err(|error| {
            issue(
                "schema.unsupported-pattern",
                path.to_vec(),
                error.to_string(),
            )
        })?;
        if !regex.is_match(string) {
            return Err(issue(
                "schema.pattern",
                path.to_vec(),
                "String does not match required pattern",
            ));
        }
    }
    if schema.get("format").and_then(Value::as_str) == Some("uri") && Url::parse(string).is_err() {
        return Err(issue("schema.uri", path.to_vec(), "Expected a valid URI"));
    }
    Ok(())
}

fn validate_number(
    schema: &Value,
    value: &Value,
    path: &[String],
    integer: bool,
) -> Result<(), ContractValidationError> {
    let number = value.as_f64().ok_or_else(|| {
        schema_issue(
            "schema.type",
            path.to_vec(),
            "Expected number",
            schema,
            value,
        )
    })?;
    if integer && number.fract() != 0.0 {
        return Err(issue("schema.integer", path.to_vec(), "Expected integer"));
    }
    if schema
        .get("minimum")
        .and_then(Value::as_f64)
        .is_some_and(|minimum| number < minimum)
        || schema
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|maximum| number > maximum)
    {
        return Err(schema_issue(
            "schema.range",
            path.to_vec(),
            "Number is outside the allowed range",
            schema,
            value,
        ));
    }
    Ok(())
}

fn check_size(
    schema: &Value,
    length: usize,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let minimum = schema
        .get("minLength")
        .or_else(|| schema.get("minItems"))
        .and_then(Value::as_u64);
    let maximum = schema
        .get("maxLength")
        .or_else(|| schema.get("maxItems"))
        .and_then(Value::as_u64);
    if minimum.is_some_and(|bound| length < bound as usize)
        || maximum.is_some_and(|bound| length > bound as usize)
    {
        return Err(issue(
            "schema.size",
            path.to_vec(),
            "Value length is outside the allowed range",
        ));
    }
    Ok(())
}
