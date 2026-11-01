use super::super::{ContractValidationError, ValidationIssue, schema_issue, union};
use super::{ValidationMode, check_size, validate_with_mode};
use serde_json::Value;

fn authored_property(root: &Value, property: &Value) -> bool {
    property["x-cursor"] != true
        && super::super::super::resolve_ref(property, &root["$defs"])["x-cursor"] != true
}

/// The shape an object schema takes, for a value of another type: its
/// fields (`{greater, less}`), a field every form pins to literals with
/// them (`type: noul|choice`) and first, or a record's key and the shape of
/// its values (`{<name>: {type: noul|choice, instructions}}`). `None` for an
/// object of any fields.
pub(super) fn object_shape(root: &Value, schema: &Value, depth: usize) -> Option<String> {
    let defs = &root["$defs"];
    let schema = super::super::super::resolve_ref(schema, defs);
    let mut forms = vec![schema];
    for keyword in ["anyOf", "oneOf"] {
        forms.extend(
            schema
                .get(keyword)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|branch| super::super::super::resolve_ref(branch, defs)),
        );
    }
    let mut fields: Vec<(&str, Vec<&str>)> = Vec::new();
    for properties in forms
        .iter()
        .filter_map(|form| form.get("properties")?.as_object())
    {
        for (name, property) in properties {
            if !authored_property(root, property) {
                continue;
            }
            let property = super::super::super::resolve_ref(property, defs);
            if property.get("not").is_some() {
                continue;
            }
            let literals = property
                .get("const")
                .into_iter()
                .chain(
                    property
                        .get("enum")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten(),
                )
                .filter_map(Value::as_str);
            let index = fields
                .iter()
                .position(|(field, _)| field == name)
                .unwrap_or_else(|| {
                    fields.push((name, Vec::new()));
                    fields.len() - 1
                });
            for literal in literals {
                if !fields[index].1.contains(&literal) {
                    fields[index].1.push(literal);
                }
            }
        }
    }
    if fields.is_empty() {
        let values = schema
            .get("additionalProperties")
            .filter(|values| values.is_object())?;
        let shown = (depth < 2)
            .then(|| object_shape(root, values, depth + 1))
            .flatten()
            .unwrap_or_else(|| "…".to_owned());
        return Some(format!("{{<name>: {shown}}}"));
    }
    fields.sort_by_key(|(_, literals)| literals.is_empty());
    let shown = fields
        .iter()
        .map(|(name, literals)| match literals.as_slice() {
            [] => (*name).to_owned(),
            _ => format!("{name}: {}", literals.join("|")),
        })
        .collect::<Vec<_>>();
    Some(format!("{{{}}}", shown.join(", ")))
}

pub(super) fn validate_object(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    // The received value is quoted only in an error: no copy on success.
    let object = match value {
        Value::Object(object) => object,
        received => {
            // A speculative probe reads only pass/fail: no shape.
            let message = match (!union::speculative())
                .then(|| object_shape(root, schema, 0))
                .flatten()
            {
                Some(shape) => format!("Expected object {shape}"),
                None => "Expected object".to_owned(),
            };
            let message = match (!union::speculative())
                .then(|| super::super::repair::object(schema, &root["$defs"], received))
                .flatten()
            {
                Some(repair) => format!("{message}; {repair}"),
                None => message,
            };
            return Err(schema_issue(
                "schema.type",
                path.clone(),
                message,
                schema,
                received,
            ));
        }
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    // A speculative probe reads only pass/fail: it stops at the first issue.
    let speculative = union::speculative();
    let mut issues = Vec::new();
    if let Err(error) = check_size(
        schema,
        ("minProperties", "maxProperties"),
        object.len(),
        path,
    ) {
        issues.extend(error.issues);
    }
    if let Some(name_schema) = schema.get("propertyNames") {
        for key in object.keys() {
            let mut name = Value::String(key.clone());
            let mut name_path = path.clone();
            name_path.push(key.clone());
            if let Err(error) =
                validate_with_mode(root, name_schema, &mut name, &mut name_path, mode)
            {
                issues.extend(error.issues);
            }
        }
    }
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        issues.extend(unknown_fields(root, properties, object, path, speculative));
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
            let result = validate_with_mode(root, additional, field, path, mode);
            path.pop();
            if let Err(error) = result {
                issues.extend(error.issues);
                if speculative {
                    return Err(ContractValidationError { issues });
                }
            }
        }
    }
    if let Some(properties) = properties {
        for (name, field_schema) in properties {
            let default = field_schema.get("default").or_else(|| {
                super::super::super::resolve_ref(field_schema, &root["$defs"]).get("default")
            });
            if mode != ValidationMode::ReadOnly
                && !object.contains_key(name)
                && let Some(default) = default
            {
                object.insert(name.clone(), default.clone());
            }
        }
    }
    issues.extend(missing_required(schema, properties, object, path));
    if speculative && !issues.is_empty() {
        return Err(ContractValidationError { issues });
    }
    if let Some(properties) = properties {
        for (name, field_schema) in properties {
            if let Some(field) = object.get_mut(name) {
                path.push(name.clone());
                let result = validate_with_mode(root, field_schema, field, path, mode);
                path.pop();
                if let Err(error) = result {
                    issues.extend(error.issues);
                    if speculative {
                        break;
                    }
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

/// An issue per field a closed object (`additionalProperties: false`)
/// does not declare, carrying what a repair needs to suggest the field
/// meant (none for a speculative probe).
fn unknown_fields(
    root: &Value,
    properties: Option<&serde_json::Map<String, Value>>,
    object: &serde_json::Map<String, Value>,
    path: &[String],
    speculative: bool,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let known_fields = || {
        serde_json::json!({
            "knownFields": properties.map(|known| known.iter()
                .filter(|(_, property)| authored_property(root, property))
                .map(|(name, _)| name).collect::<Vec<_>>()).unwrap_or_default()
        })
    };
    for key in object.keys() {
        if !properties.is_some_and(|known| known.contains_key(key)) {
            let mut field_path = path.to_vec();
            field_path.push(key.clone());
            issues.push(ValidationIssue {
                rule_id: "schema.unknown-field".into(),
                path: field_path,
                message: format!("Unknown field: {key}"),
                // Embed known fields so format_input_error can suggest alternatives.
                schema: (!speculative).then(known_fields),
                received: None,
            });
        }
    }
    issues
}

/// An issue per field the schema's `required` lists that `object` lacks.
fn missing_required(
    schema: &Value,
    properties: Option<&serde_json::Map<String, Value>>,
    object: &serde_json::Map<String, Value>,
    path: &[String],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(required) {
            let mut field_path = path.to_vec();
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
    issues
}
