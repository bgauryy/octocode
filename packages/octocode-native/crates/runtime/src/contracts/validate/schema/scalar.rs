use super::super::{ContractValidationError, issue, schema_issue, shape, union};
use super::{
    ValidationMode, compiled_pattern, line_range_message, pattern_message, validate_with_mode,
};
use serde_json::Value;
use url::Url;

pub(super) fn validate_array(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    path: &mut Vec<String>,
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    let array = match value {
        Value::Array(array) => array,
        received => {
            // Admission never repairs a value: name the fix, and never
            // suggest wrapping an encoded list or an item the list rejects.
            let message = match &*received {
                Value::String(text) if shape::looks_like_json_array(text) => {
                    "Expected array; send a JSON array, not a JSON-encoded string".to_owned()
                }
                Value::String(_) if shape::items_accept(root, schema, received) => {
                    format!("Expected array; wrap the value: [{received}]")
                }
                _ => "Expected array".to_owned(),
            };
            return Err(schema_issue(
                "schema.type",
                path.clone(),
                &message,
                schema,
                received,
            ));
        }
    };
    let mut issues = Vec::new();
    if let Err(error) = check_size(schema, ("minItems", "maxItems"), array.len(), path) {
        issues.extend(error.issues);
    }
    if let Some(items) = schema.get("items") {
        for (index, item) in array.iter_mut().enumerate() {
            path.push(index.to_string());
            let result = validate_with_mode(root, items, item, path, mode);
            path.pop();
            if let Err(error) = result {
                issues.extend(error.issues);
                // A speculative probe reads only pass/fail.
                if union::speculative() {
                    break;
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

pub(super) fn validate_string(
    schema: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let line_range =
        schema.get("pattern").and_then(Value::as_str) == Some(shape::LINE_RANGE_PATTERN);
    let string = value.as_str().ok_or_else(|| {
        if line_range {
            return schema_issue(
                "schema.type",
                path.to_vec(),
                line_range_message(value, path),
                schema,
                value,
            );
        }
        // A list for a row field that takes one string: each value is its
        // own row of the batch.
        let message = match (path, value) {
            ([queries, _, _], Value::Array(items))
                if queries == "queries" && items.iter().all(Value::is_string) =>
            {
                "Expected one string, not a list; send one queries[] row per value".to_owned()
            }
            // A count range (`x-countRange`) reads a bare count as exactly
            // that many: name both readings of the number sent.
            (_, Value::Number(count)) if schema.get("x-countRange").is_some() => {
                format!("Expected string; send \">={count}\" for at least {count}, or \"{count}\" for exactly {count}")
            }
            _ => "Expected string".to_owned(),
        };
        schema_issue("schema.type", path.to_vec(), message, schema, value)
    })?;
    check_size(
        schema,
        ("minLength", "maxLength"),
        string.chars().count(),
        path,
    )?;
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        let regex = compiled_pattern(pattern)
            .map_err(|error| issue("schema.unsupported-pattern", path.to_vec(), error))?;
        if regex.find(string).is_none() {
            let message = if line_range {
                line_range_message(value, path)
            } else {
                pattern_message(string, path, pattern, &regex, schema)
            };
            // The schema and value let the repair read the intended value
            // (a line-range typo, a joined `owner/repo` pair).
            return Err(schema_issue(
                "schema.pattern",
                path.to_vec(),
                message,
                schema,
                value,
            ));
        }
    }
    if schema.get("format").and_then(Value::as_str) == Some("uri") && Url::parse(string).is_err() {
        return Err(issue("schema.uri", path.to_vec(), "Expected a valid URI"));
    }
    Ok(())
}

pub(super) fn validate_number(
    schema: &Value,
    value: &Value,
    path: &[String],
    integer: bool,
) -> Result<(), ContractValidationError> {
    let number = value.as_f64().ok_or_else(|| {
        let message = match value.as_str() {
            Some(text) if text.trim().parse::<f64>().is_ok() => {
                format!("Expected number; send {}, not \"{text}\"", text.trim())
            }
            _ => "Expected number".to_owned(),
        };
        schema_issue("schema.type", path.to_vec(), message, schema, value)
    })?;
    if integer && number.fract() != 0.0 {
        return Err(issue("schema.integer", path.to_vec(), "Expected integer"));
    }
    let minimum = schema.get("minimum").and_then(Value::as_f64);
    let maximum = schema.get("maximum").and_then(Value::as_f64);
    if minimum.is_some_and(|minimum| number < minimum)
        || maximum.is_some_and(|maximum| number > maximum)
    {
        // Name the bounds so the caller can fix the value in one step.
        let bound = |value: f64| {
            Value::from(value)
                .to_string()
                .trim_end_matches(".0")
                .to_owned()
        };
        let range = match (minimum, maximum) {
            (Some(minimum), Some(maximum)) => format!("{}-{}", bound(minimum), bound(maximum)),
            (Some(minimum), None) => format!(">= {}", bound(minimum)),
            (None, Some(maximum)) => format!("<= {}", bound(maximum)),
            (None, None) => String::new(),
        };
        return Err(schema_issue(
            "schema.range",
            path.to_vec(),
            format!("Number is outside the allowed range ({range})"),
            schema,
            value,
        ));
    }
    Ok(())
}

pub(super) fn check_size(
    schema: &Value,
    keys: (&str, &str),
    length: usize,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let minimum = schema.get(keys.0).and_then(Value::as_u64);
    let maximum = schema.get(keys.1).and_then(Value::as_u64);
    if let Some(bound) = minimum.filter(|bound| length < *bound as usize) {
        return Err(issue(
            "schema.size",
            path.to_vec(),
            format!("Value length {length} is below the minimum of {bound}"),
        ));
    }
    if let Some(bound) = maximum.filter(|bound| length > *bound as usize) {
        return Err(issue(
            "schema.size",
            path.to_vec(),
            format!("Value length {length} exceeds the maximum of {bound}"),
        ));
    }
    Ok(())
}

/// Agents often send a boolean for an on/off enum (`regex:true`,
/// `minify:false`); name the value that means what they intended.
pub(in super::super) fn boolean_enum_hint(values: &[Value], received: &Value) -> Option<String> {
    let pick = boolean_enum_value(values, received)?;
    Some(format!(" — booleans are not accepted; use \"{pick}\""))
}

/// The enum value a boolean means: `false` the off value (`none`,
/// `literal`, `off`), `true` the first other value.
pub(in super::super) fn boolean_enum_value<'a>(
    values: &'a [Value],
    received: &Value,
) -> Option<&'a str> {
    let on = received.as_bool()?;
    let off = ["none", "literal", "off"]
        .iter()
        .find(|name| values.iter().any(|v| v.as_str() == Some(**name)));
    if on {
        values
            .iter()
            .filter_map(Value::as_str)
            .find(|v| Some(v) != off)
    } else {
        let off = off?;
        values.iter().filter_map(Value::as_str).find(|v| v == off)
    }
}
