use super::super::{ContractValidationError, ValidationIssue, schema_issue, union};
use super::{ValidationMode, check_size, span_field, validate_with_mode};
use serde_json::Value;

/// The known fields whose `enum` or `const` lists `value` (a string): an
/// unknown field carrying one of those values most likely meant one of them
/// (`{"type":"match"}` for `operation`), and an unknown field set to `true`
/// whose name one of them lists meant that value (`merged:true` for
/// `state:"merged"`).
pub(in super::super) fn fields_listing(
    root: &Value,
    properties: &serde_json::Map<String, Value>,
    value: &Value,
) -> Vec<String> {
    if !value.is_string() {
        return Vec::new();
    }
    let lists = |schema: &Value| {
        let schema = super::super::super::resolve_ref(schema, &root["$defs"]);
        schema.get("const") == Some(value)
            || schema
                .get("enum")
                .and_then(Value::as_array)
                .is_some_and(|values| values.contains(value))
    };
    properties
        .iter()
        .filter(|(_, schema)| lists(schema))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The known fields whose `x-valueKinds` (core metadata: the kinds of value
/// the field takes) lists `name`: an unknown field named for a kind of value
/// (`branch:"main"` for `ref`) most likely meant that field.
pub(in super::super) fn fields_of_kind(
    root: &Value,
    properties: &serde_json::Map<String, Value>,
    name: &str,
) -> Vec<String> {
    properties
        .iter()
        .filter(|(_, schema)| {
            super::super::super::resolve_ref(schema, &root["$defs"])
                .get("x-valueKinds")
                .and_then(Value::as_array)
                .is_some_and(|kinds| {
                    kinds
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|kind| kind.eq_ignore_ascii_case(name))
                })
        })
        .map(|(field, _)| field.clone())
        .collect()
}

/// The known fields whose `x-covers` (core metadata: names a caller may use
/// for the constraint the field expresses) lists `name`: an unknown field
/// so named (`limit` for `pageSize`) is pointed at that field.
fn fields_covering(
    root: &Value,
    properties: &serde_json::Map<String, Value>,
    name: &str,
) -> Vec<String> {
    properties
        .iter()
        .filter(|(_, schema)| {
            super::super::super::resolve_ref(schema, &root["$defs"])
                .get("x-covers")
                .and_then(Value::as_array)
                .is_some_and(|names| names.iter().any(|covered| covered.as_str() == Some(name)))
        })
        .map(|(field, _)| field.clone())
        .collect()
}

/// The shape an object schema takes, for a value of another type: its
/// fields (`{greater, less}`), a field every form pins to literals with
/// them (`type: noul|choice`) and first, or a record's key and the shape of
/// its values (`{<name>: {type: noul|choice, instructions}}`). `None` for an
/// object of any fields.
fn object_shape(root: &Value, schema: &Value, depth: usize) -> Option<String> {
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
        // Built on the first unknown field only; a speculative probe skips it.
        let row_fields = || object.keys().cloned().collect::<Vec<_>>();
        let known_fields = |key: &str, value: &Value| -> Value {
            let known: Vec<Value> = properties
                .map(|p| p.keys().map(|k| serde_json::json!(k)).collect())
                .unwrap_or_default();
            let listing = |value: &Value| {
                properties
                    .map(|p| fields_listing(root, p, value))
                    .unwrap_or_default()
            };
            serde_json::json!({
                "knownFields": known,
                "valueFields": listing(value),
                "nameFields": if value == &Value::Bool(true) { listing(&Value::from(key)) } else { Vec::new() },
                "kindFields": properties
                    .map(|p| fields_of_kind(root, p, key))
                    .unwrap_or_default(),
                "coverFields": properties
                    .map(|p| fields_covering(root, p, key))
                    .unwrap_or_default(),
                "rowFields": row_fields(),
                "spanField": properties.and_then(|p| span_field(root, p)),
            })
        };
        for (key, field) in object.iter() {
            if !properties.is_some_and(|known| known.contains_key(key)) {
                let mut field_path = path.clone();
                field_path.push(key.clone());
                issues.push(ValidationIssue {
                    rule_id: "schema.unknown-field".into(),
                    path: field_path,
                    message: format!("Unknown field: {key}"),
                    // Embed known fields so format_input_error can suggest alternatives.
                    schema: (!speculative).then(|| known_fields(key, field)),
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
            // A shared field (`$ref`) declares its default on the definition.
            let field_schema = super::super::super::resolve_ref(field_schema, &root["$defs"]);
            if !object.contains_key(name)
                && let Some(default) = field_schema.get("default")
            {
                object.insert(name.clone(), default.clone());
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
    if mode == ValidationMode::Input
        && let Some(properties) = properties
    {
        super::super::repository::settle(root, properties, object, path, &mut issues);
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}
