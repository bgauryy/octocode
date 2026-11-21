use super::super::{ContractValidationError, ValidationIssue, schema_issue, union};
use super::{ValidationMode, check_size, validate_with_mode};
use serde_json::Value;

/// Whether a caller authors `property`: cursor state (`x-cursor`) is not.
pub(in super::super) fn authored_property(root: &Value, property: &Value) -> bool {
    crate::contracts::schema_facts::schema_keyword(property, &root["$defs"], "x-cursor")
        != Some(&Value::Bool(true))
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

/// For an object sent a scalar that several of its fields would take (a
/// rule sent as the text of its pattern, kind or regex): the scalar is one
/// field's value and nothing says which, so the repair names the fields,
/// those of every one of the object's `forms`. `None` when no field takes
/// it.
fn one_field_value(
    root: &Value,
    forms: &[&Value],
    received: &Value,
) -> Result<Option<String>, ContractValidationError> {
    if !(received.is_string() || received.is_number() || received.is_boolean()) {
        return Ok(None);
    }
    let defs = &root["$defs"];
    let _speculation = union::Speculation::enter();
    let mut fields: Vec<&str> = Vec::new();
    for form in forms {
        let Some(properties) = super::super::super::resolve_ref(form, defs)
            .get("properties")
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (name, property) in properties {
            if !authored_property(root, property) || fields.contains(&name.as_str()) {
                continue;
            }
            let taken = super::probe(validate_with_mode(
                root,
                property,
                &mut received.clone(),
                &mut Vec::new(),
                ValidationMode::ReadOnly,
            ))?;
            if taken {
                fields.push(name.as_str());
            }
        }
    }
    if fields.is_empty() {
        return Ok(None);
    }
    let received = super::super::repair::shown(received);
    Ok(Some(format!(
        "{received} is the value of one field: send {{\"<field>\":{received}}} with <field> one of {}",
        fields.join(", ")
    )))
}

/// The message for a value that is no object: the expected shape, then the
/// one repair that reads it. `forms` are the object schemas the value may
/// take: `schema` alone, or every form of the union `schema` composes, so
/// the shape lists each literal the forms pin a field to and the repair is
/// the first a form offers. A speculative probe reads only pass/fail: no
/// shape and no repair.
fn not_an_object(
    root: &Value,
    schema: &Value,
    forms: &[&Value],
    received: &Value,
) -> Result<String, ContractValidationError> {
    if union::speculative() {
        return Ok("Expected object".to_owned());
    }
    let message = match object_shape(root, schema, 0) {
        Some(shape) => format!("Expected object {shape}"),
        None => "Expected object".to_owned(),
    };
    let repair = match forms
        .iter()
        .find_map(|form| super::super::repair::object(form, &root["$defs"], received))
    {
        Some(fix) => Some(fix),
        None => one_field_value(root, forms, received)?,
    };
    Ok(match repair {
        Some(repair) => format!("{message}; {repair}"),
        None => message,
    })
}

/// A value that is no object, refused by a union whose every form is an
/// object: the form the union's diagnosis chose says only its own shape
/// (`{type: noul, …}`), so its issue at `path` takes the union's, which
/// names every literal the forms pin a field to (`type:
/// noul|choice|score|box`). Any other failure is left as it is.
pub(in super::super) fn name_object_forms(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    received: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    if received.is_object() || branches.len() < 2 {
        return Ok(());
    }
    let defs = &root["$defs"];
    let forms = branches
        .iter()
        .map(|branch| super::super::super::resolve_ref(branch, defs))
        .collect::<Vec<_>>();
    if !forms
        .iter()
        .all(|form| form.get("type").and_then(Value::as_str) == Some("object"))
    {
        return Ok(());
    }
    let Some(issue) = issues
        .iter_mut()
        .find(|issue| issue.rule_id == "schema.type" && issue.path == path)
    else {
        return Ok(());
    };
    let whole = serde_json::json!({ "anyOf": branches });
    issue.message = not_an_object(root, &whole, &forms, received)?;
    Ok(())
}

/// What a `schema.required` issue says before any stage adds why the field
/// is required here.
pub(in crate::contracts::validate) fn missing_required_text(field: &str) -> String {
    format!("Missing required field: {field}")
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
            let overflow = (!union::speculative())
                .then(|| super::super::repair::selection_overflow(schema, &root["$defs"], received))
                .flatten();
            if let Some(message) = overflow {
                return Err(schema_issue(
                    "schema.size",
                    path.clone(),
                    message,
                    schema,
                    received,
                ));
            }
            let message = not_an_object(root, schema, &[schema], received)?;
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
    let mut patterned = std::collections::HashSet::new();
    if let Some(patterns) = schema.get("patternProperties").and_then(Value::as_object) {
        for (pattern, field_schema) in patterns {
            let pattern = super::compiled_pattern(pattern).map_err(super::super::internal)?;
            for (key, field) in object.iter_mut() {
                if pattern.find(key).is_none() {
                    continue;
                }
                patterned.insert(key.clone());
                path.push(key.clone());
                let result = validate_with_mode(root, field_schema, field, path, mode);
                path.pop();
                if let Err(error) = result {
                    issues.extend(error.issues);
                    if speculative {
                        return Err(ContractValidationError { issues });
                    }
                }
            }
        }
    }
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        issues.extend(unknown_fields(
            root,
            properties,
            &patterned,
            object,
            path,
            speculative,
        ));
    }
    if let Some(additional) = schema
        .get("additionalProperties")
        .filter(|value| value.is_object())
    {
        for (key, field) in object.iter_mut() {
            if properties.is_some_and(|known| known.contains_key(key)) || patterned.contains(key) {
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
            let default = crate::contracts::schema_facts::schema_keyword(
                field_schema,
                &root["$defs"],
                "default",
            );
            if mode != ValidationMode::ReadOnly
                && !object.contains_key(name)
                && let Some(default) = default
            {
                object.insert(name.clone(), default.clone());
            }
        }
    }
    issues.extend(missing_required(root, schema, properties, object, path));
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
    patterned: &std::collections::HashSet<String>,
    object: &serde_json::Map<String, Value>,
    path: &[String],
    speculative: bool,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let sent_fields = object.keys().collect::<Vec<_>>();
    let known_fields = || {
        serde_json::json!({
            // What the object already carries: a repair never suggests it.
            "sentFields": sent_fields,
            "knownFields": properties.map(|known| known.iter()
                .filter(|(_, property)| authored_property(root, property))
                .map(|(name, _)| name).collect::<Vec<_>>()).unwrap_or_default()
        })
    };
    for key in object.keys() {
        if !properties.is_some_and(|known| known.contains_key(key)) && !patterned.contains(key) {
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
    root: &Value,
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
                    missing_required_text(required),
                    crate::contracts::resolve_ref(
                        properties
                            .and_then(|items| items.get(required))
                            .unwrap_or(&Value::Null),
                        &root["$defs"],
                    ),
                    &Value::Null,
                )
                .issues,
            );
        }
    }
    issues
}
