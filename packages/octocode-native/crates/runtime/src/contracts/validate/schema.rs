//! Generic JSON-schema validation.
//!
//! Tool-agnostic structural validation of a value against a (sub)schema:
//! `$ref`/`not`/`oneOf`/`anyOf` resolution, `const`/`enum`, and the
//! object/array/string/number/size checks. Per-tool rule logic lives in the
//! parent module; union-branch selection lives in `super::union`.

use super::{ContractValidationError, ValidationIssue, issue, schema_issue, shape, union};
use regress::Regex;
use serde_json::Value;
use url::Url;

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
fn span_field(root: &Value, properties: &serde_json::Map<String, Value>) -> Option<String> {
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

/// The known fields whose `enum` or `const` lists `value` (a string): an
/// unknown field carrying one of those values most likely meant one of them
/// (`{"type":"match"}` for `operation`), and an unknown field set to `true`
/// whose name one of them lists meant that value (`merged:true` for
/// `state:"merged"`).
pub(super) fn fields_listing(
    root: &Value,
    properties: &serde_json::Map<String, Value>,
    value: &Value,
) -> Vec<String> {
    if !value.is_string() {
        return Vec::new();
    }
    let lists = |schema: &Value| {
        let schema = super::super::resolve_ref(schema, &root["$defs"]);
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
pub(super) fn fields_of_kind(
    root: &Value,
    properties: &serde_json::Map<String, Value>,
    name: &str,
) -> Vec<String> {
    properties
        .iter()
        .filter(|(_, schema)| {
            super::super::resolve_ref(schema, &root["$defs"])
                .get("x-valueKinds")
                .and_then(Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some(name)))
        })
        .map(|(field, _)| field.clone())
        .collect()
}

fn validate_object(
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
            return Err(schema_issue(
                "schema.type",
                path.clone(),
                "Expected object",
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
            let field_schema = super::super::resolve_ref(field_schema, &root["$defs"]);
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
        super::repository::settle(root, properties, object, path, &mut issues);
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

fn validate_string(
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
                "Expected one string, not a list; send one queries[] row per value"
            }
            _ => "Expected string",
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

/// Compiled schema patterns, keyed by source. Patterns come from the embedded
/// contract (a small fixed set), so the cache stays bounded. The ECMAScript
/// engine matches generated wire types; cache hits reuse the compiled pattern.
type PatternCache = std::collections::HashMap<String, Result<std::sync::Arc<Regex>, String>>;

fn pattern_cache() -> &'static std::sync::Mutex<PatternCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<PatternCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn compiled_pattern(pattern: &str) -> Result<std::sync::Arc<Regex>, String> {
    let mut cache = pattern_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(compiled) = cache.get(pattern) {
        return compiled.clone();
    }
    let compiled = Regex::new(pattern)
        .map(std::sync::Arc::new)
        .map_err(|error| error.to_string());
    cache.insert(pattern.to_owned(), compiled.clone());
    compiled
}

/// The longest pattern an error quotes as the expected form.
const READABLE_PATTERN_CHARS: usize = 40;

/// A blank value fails the non-blank patterns (`\S`) paths and names use
/// (a blank brief is dropped before validation), a search `qualifiers`
/// string fails on one term, and any other value names its field and the
/// pattern it must match (a long pattern gives the field's note instead).
fn pattern_message(
    value: &str,
    path: &[String],
    pattern: &str,
    regex: &Regex,
    schema: &Value,
) -> String {
    if path.last().map(String::as_str) == Some("qualifiers")
        && let Some(term) = super::qualifiers::qualifier_terms(value)
            .into_iter()
            .find(|term| regex.find(&quoted_term(term)).is_none())
    {
        return match term.strip_prefix('-') {
            Some(positive) if regex.find(&quoted_term(positive)).is_some() => format!(
                "\"{term}\": negation is not supported for this filter; drop the leading -."
            ),
            _ => format!(
                "\"{term}\" is not an allowed key:value filter; {}put free text in keywords and scope in owner/repo.",
                qualifier_keys_note(schema)
            ),
        };
    }
    if !value.trim().is_empty() {
        let shown: String = value.chars().take(80).collect();
        let field = path
            .iter()
            .rev()
            .find(|segment| segment.parse::<usize>().is_err());
        // A short pattern is the expected form; a pattern too long to read
        // falls back to the field's own note, when it has one. A
        // description of a field with a short pattern is usage prose.
        return match (field, schema.get("description").and_then(Value::as_str)) {
            (_, Some(note)) if pattern.chars().count() > READABLE_PATTERN_CHARS => {
                format!("\"{shown}\" does not match the expected form: {note}")
            }
            (Some(field), _) => {
                format!("\"{shown}\" is not a valid {field}: it must match {pattern}")
            }
            (None, _) => format!("\"{shown}\" does not match {pattern}"),
        };
    }
    "is empty; give non-blank text.".into()
}

/// A rejected line-range item: the value as sent, the span it spells when
/// it spells one (`did you mean "3-6"?`), then the shape once
/// (`ranges: ["299-342"] (strings "start-end")`), named by its field.
fn line_range_message(value: &Value, path: &[String]) -> String {
    let field = path
        .iter()
        .rev()
        .find(|segment| segment.parse::<usize>().is_err())
        .map_or("ranges", String::as_str);
    let clip = |text: &str| {
        let shown: String = text.chars().take(80).collect();
        if shown.len() < text.len() {
            format!("{shown}…")
        } else {
            shown
        }
    };
    let sent = match value {
        Value::String(text) => serde_json::to_string(&clip(text)).unwrap_or_default(),
        other => clip(&other.to_string()),
    };
    let reading = shape::line_range_reading(value)
        .map(|span| format!(" (did you mean \"{span}\"?)"))
        .unwrap_or_default();
    format!("{sent} is not a line range{reading}; {field}: [\"299-342\"] (strings \"start-end\")")
}

/// The allowed keys a `qualifiers` field declares (`x-qualifierKeys`, a
/// contract keyword the published view drops): `allowed keys: a, b; e.g.
/// "a:x"; `, empty when the field declares none.
fn qualifier_keys_note(schema: &Value) -> String {
    let keys = schema
        .get("x-qualifierKeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    match keys.first() {
        Some(first) => format!("allowed keys: {}; e.g. \"{first}:x\"; ", keys.join(", ")),
        None => String::new(),
    }
}

/// A split qualifier term as it was written: a value with spaces is quoted.
fn quoted_term(term: &str) -> String {
    match term.split_once(':') {
        Some((key, value)) if value.contains(char::is_whitespace) => format!("{key}:\"{value}\""),
        _ => term.to_owned(),
    }
}

fn validate_number(
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

fn check_size(
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
pub(super) fn boolean_enum_hint(values: &[Value], received: &Value) -> Option<String> {
    let pick = boolean_enum_value(values, received)?;
    Some(format!(" — booleans are not accepted; use \"{pick}\""))
}

/// The enum value a boolean means: `false` the off value (`none`,
/// `literal`, `off`), `true` the first other value.
pub(super) fn boolean_enum_value<'a>(values: &'a [Value], received: &Value) -> Option<&'a str> {
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

#[cfg(test)]
mod tests {
    use super::validate_schema;
    use serde_json::{Value, json};

    #[test]
    fn nested_refinement_does_not_hide_the_question_discriminator() {
        let schema = serde_json::json!({"anyOf":[
            {"type":"object","properties":{"type":{"const":"noul"}},
             "anyOf":[{"required":["instructions"]},{"required":["criteria"]}]},
            {"type":"object","properties":{"type":{"const":"choice"}},"required":["criteria"]}
        ]});
        let mut value = serde_json::json!({"type":"choice"});
        let error = validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap_err();
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(error.issues[0].path, vec!["criteria"]);
    }

    #[test]
    fn union_guards_preserve_enclosing_required_fields_and_property_validation() {
        let schema = json!({
            "type":"object", "additionalProperties":false,
            "properties":{
                "state":{"type":["string","object","array"]},
                "source":{"type":"object"},
                "questions":{"type":"array","minItems":1}
            },
            "required":["questions"],
            "oneOf":[{"required":["state"]},{"required":["source"]}]
        });
        for mut value in [
            json!({"state":"text","questions":[{}]}),
            json!({"state":[],"questions":[{}]}),
            json!({"state":{},"questions":[{}]}),
            json!({"source":{},"questions":[{}]}),
        ] {
            assert!(
                validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
                "{value}"
            );
        }
        for mut value in [
            json!({"state":"text"}),
            json!({"state":"text","questions":[]}),
            json!({"state":"text","questions":[{}],"unknown":true}),
            json!({"state":"text","source":{},"questions":[{}]}),
            json!({"questions":[{}]}),
            json!({"state":1,"questions":[{}]}),
            json!({"state":"text","questions":"encoded"}),
        ] {
            assert!(
                validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn successful_union_branch_defaults_reach_sibling_constraints() {
        let schema = json!({
            "type":"object", "required":["mode"],
            "anyOf":[{"type":"object","properties":{"mode":{"type":"string","default":"read"}}}],
            "properties":{"mode":{"const":"read"}}, "additionalProperties":false
        });
        let mut value = json!({});
        validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap();
        assert_eq!(value, json!({"mode":"read"}));
        assert!(
            validate_schema(&schema, &schema, &mut json!({"mode":"write"}), &mut vec![]).is_err()
        );
    }

    #[test]
    fn reference_chains_validate_sibling_constraints_at_every_level() {
        let schema = json!({
            "$defs":{
                "base":{"type":"integer"},
                "bounded":{"$ref":"#/$defs/base","minimum":2}
            },
            "$ref":"#/$defs/bounded", "maximum":3
        });
        for (mut value, valid) in [
            (json!(1), false),
            (json!(2), true),
            (json!(3), true),
            (json!(4), false),
            (json!(2.5), false),
            (json!("2"), false),
        ] {
            assert_eq!(
                validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
                valid,
                "{value}"
            );
        }
        let cyclic =
            json!({"$defs":{"a":{"$ref":"#/$defs/b"},"b":{"$ref":"#/$defs/a"}},"$ref":"#/$defs/a"});
        assert_eq!(
            validate_schema(&cyclic, &cyclic, &mut json!(1), &mut vec![])
                .unwrap_err()
                .issues[0]
                .rule_id,
            "schema.unsupported-ref"
        );
    }

    #[test]
    fn adjacent_unions_and_literal_constraints_all_apply() {
        let schema =
            json!({"oneOf":[{"const":"a"},{"const":"b"}],"anyOf":[{"const":"a"},{"const":"c"}]});
        assert!(validate_schema(&schema, &schema, &mut json!("a"), &mut vec![]).is_ok());
        for mut value in [json!("b"), json!("c")] {
            assert!(validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err());
        }
        let schema = json!({"anyOf":[{"type":"string"}],"const":"a"});
        assert!(validate_schema(&schema, &schema, &mut json!("b"), &mut vec![]).is_err());
    }

    #[test]
    fn type_arrays_enforce_members_and_only_applicable_bounds() {
        let schema = json!({"type":["string","array","object","null"],"minLength":2,"maxItems":1,"minProperties":1});
        for (mut value, valid) in [
            (json!("ab"), true),
            (json!("a"), false),
            (json!([]), true),
            (json!([1, 2]), false),
            (json!({"a":1,"b":2}), true),
            (json!({}), false),
            (json!(null), true),
            (json!(1), false),
            (json!(false), false),
        ] {
            assert_eq!(
                validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
                valid,
                "{value}"
            );
        }
        for (schema, mut value, valid) in [
            (
                json!({"type":["integer","null"],"minimum":1}),
                json!(1),
                true,
            ),
            (json!({"type":["integer","null"]}), json!(1.5), false),
            (json!({"type":["integer","number"]}), json!(1.5), true),
            (json!({"type":["boolean","null"]}), json!(false), true),
            (json!({"type":["string","unsupported"]}), json!("a"), false),
            (json!({"type":["string",1]}), json!("a"), false),
            (json!({"type":[]}), json!("a"), false),
        ] {
            assert_eq!(
                validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
                valid,
                "{schema}: {value}"
            );
        }
    }

    /// Every `pattern` the contract declares compiles with the engine the
    /// validator runs. One that does not (a look-around, say) rejects every
    /// value of its field as the caller's invalid input, so the field is
    /// unusable; it is a contract defect and fails here instead.
    #[test]
    fn every_contract_pattern_compiles_with_the_validator_engine() {
        fn patterns<'a>(value: &'a Value, at: String, out: &mut Vec<(String, &'a str)>) {
            match value {
                Value::Object(map) => {
                    for (key, child) in map {
                        if key == "pattern"
                            && let Some(pattern) = child.as_str()
                        {
                            out.push((at.clone(), pattern));
                        }
                        patterns(child, format!("{at}/{key}"), out);
                    }
                }
                Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        patterns(child, format!("{at}/{index}"), out);
                    }
                }
                _ => {}
            }
        }
        let contract: Value =
            serde_json::from_str(crate::contracts::contract_json()).expect("contract");
        let mut found = Vec::new();
        patterns(&contract, String::new(), &mut found);
        assert!(!found.is_empty(), "the contract declares patterns");
        let broken: Vec<_> = found
            .into_iter()
            .filter_map(|(at, pattern)| {
                super::compiled_pattern(pattern)
                    .err()
                    .map(|error| format!("{at}: {pattern}: {}", error.lines().last().unwrap_or("")))
            })
            .collect();
        assert!(
            broken.is_empty(),
            "uncompilable contract patterns:\n{}",
            broken.join("\n")
        );
    }

    #[test]
    fn boolean_for_an_on_off_enum_names_the_intended_value() {
        let regex = json!({"type":"string","enum":["literal","rust","pcre2"]});
        let minify = json!({"type":"string","enum":["none","standard","symbols"]});
        let message = |schema: &Value, mut value: Value| {
            validate_schema(schema, schema, &mut value, &mut vec![])
                .unwrap_err()
                .issues[0]
                .message
                .clone()
        };
        assert!(message(&regex, json!(true)).ends_with("use \"rust\""));
        assert!(message(&regex, json!(false)).ends_with("use \"literal\""));
        assert!(message(&minify, json!(false)).ends_with("use \"none\""));
        assert!(!message(&regex, json!("x")).contains("booleans"));
    }

    #[test]
    fn bounded_records_enforce_property_counts_and_values() {
        let schema = json!({"type":"object", "minProperties":1, "maxProperties":24,
            "propertyNames":{"type":"string", "pattern":"^[a-z][a-zA-Z0-9_]{0,39}$"},
            "additionalProperties":{"type":"string", "minLength":1, "maxLength":800}});
        for count in [0, 1, 24, 25] {
            let mut value: Value = (0..count)
                .map(|index| (format!("claim{index}"), json!("A bounded claim.")))
                .collect::<serde_json::Map<_, _>>()
                .into();
            let result = validate_schema(&schema, &schema, &mut value, &mut vec![]);
            assert_eq!(result.is_ok(), (1..=24).contains(&count));
            if let Err(error) = result {
                assert!(
                    error
                        .issues
                        .iter()
                        .any(|issue| issue.rule_id == "schema.size")
                );
            }
        }
        for mut value in [
            json!({"Bad id":"claim"}),
            json!({"claim":""}),
            json!({"claim":1}),
        ] {
            assert!(validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err());
        }
    }

    fn pattern_cached(pattern: &str) -> bool {
        super::pattern_cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(pattern)
    }

    /// Contract patterns are a fixed set; compiling one per validated string
    /// is a per-response cost, since every continuation is validated against
    /// every tool's query schema.
    #[test]
    fn schema_patterns_compile_once_per_process() {
        let pattern = "^cache-probe-[a-z]{3}$";
        let schema = json!({"type":"string","pattern":pattern});
        for value in ["cache-probe-abc", "cache-probe-xyz", "nope"] {
            let _ = validate_schema(&schema, &schema, &mut json!(value), &mut vec![]);
        }
        assert!(pattern_cached(pattern));
        let mut bad = json!("cache-probe-ab");
        assert!(validate_schema(&schema, &schema, &mut bad, &mut vec![]).is_err());
    }

    /// A rejected line range names the value sent and the shape
    /// once, whether it is the wrong text or not a string at all.
    #[test]
    fn rejected_line_ranges_quote_the_shape() {
        let schema = json!({"type":"array","items":{"type":"string","pattern":super::shape::LINE_RANGE_PATTERN}});
        let message = |mut value: Value| {
            let mut path = vec!["queries".to_owned(), "0".to_owned(), "ranges".to_owned()];
            validate_schema(&schema, &schema, &mut value, &mut path)
                .unwrap_err()
                .issues[0]
                .message
                .clone()
        };
        assert_eq!(
            message(json!(["[&quot;299-342&quot;]"])),
            r#""[&quot;299-342&quot;]" is not a line range; ranges: ["299-342"] (strings "start-end")"#
        );
        assert_eq!(
            message(json!([{"s":"12-47"}])),
            r#"{"s":"12-47"} is not a line range; ranges: ["299-342"] (strings "start-end")"#
        );
    }

    /// A rejected qualifier term lists the keys the field declares.
    #[test]
    fn rejected_qualifier_lists_the_declared_keys() {
        let schema = json!({"type":"string","pattern":"^\\s*(?:(?:author|label):(?:\"[^\"]+\"|[^\\s\"]+)\\s*)+$",
            "x-qualifierKeys":["author","label"]});
        let message = |schema: &Value| {
            validate_schema(
                schema,
                schema,
                &mut json!("linked:issue"),
                &mut vec!["qualifiers".into()],
            )
            .unwrap_err()
            .issues[0]
                .message
                .clone()
        };
        let listed = message(&schema);
        assert!(listed.contains("allowed keys: author, label"), "{listed}");
        assert!(listed.contains("\"linked:issue\""), "{listed}");
        let mut bare = schema.clone();
        bare.as_object_mut()
            .expect("schema")
            .remove("x-qualifierKeys");
        assert!(!message(&bare).contains("allowed keys"));
    }
}
