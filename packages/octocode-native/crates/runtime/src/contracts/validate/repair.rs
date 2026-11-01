//! The repair half of a type or name error: the value the caller sent,
//! rendered in the shape the field takes, or the field or value it most
//! likely meant. Each is a clause appended to the error line, so one line
//! says what is wrong and what to send instead.
use serde_json::Value;

/// Widest received value a repair quotes; a longer one is elided.
const SHOWN_CHARS: usize = 60;

/// `value` as compact JSON, elided past [`SHOWN_CHARS`].
fn shown(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= SHOWN_CHARS {
        return text;
    }
    match value {
        Value::Object(_) => "{…}".to_owned(),
        Value::Array(_) => "[…]".to_owned(),
        Value::String(_) => "\"…\"".to_owned(),
        _ => text,
    }
}

/// A string whose text parses as JSON of the kind the field takes.
fn holds_json(received: &Value, wanted: fn(&Value) -> bool) -> bool {
    received
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .is_some_and(|parsed| wanted(&parsed))
}

const SEND_ITSELF: &str = "send the JSON value itself, not a string holding it";

/// For an array field: unquote a JSON string, wrap a value that is one
/// item (`fits`), else name the item shape.
pub(super) fn array(received: &Value, fits: bool, item_shape: Option<&str>) -> String {
    if holds_json(received, Value::is_array) {
        return format!("Expected a JSON array; {SEND_ITSELF}");
    }
    if fits {
        let value = shown(received);
        return format!("Expected a JSON array; send [{value}], not {value}");
    }
    match item_shape {
        Some(shape) => format!("Expected a JSON array of {shape}"),
        None => "Expected a JSON array".to_owned(),
    }
}

/// The repaired `queries` value: an object is one row, a string may hold
/// the array.
pub(crate) fn queries(received: &Value) -> String {
    if holds_json(received, Value::is_array) {
        return SEND_ITSELF.to_owned();
    }
    let value = shown(received);
    match received {
        Value::Object(_) => "send \"queries\":[{…}], not \"queries\":{…}".to_owned(),
        _ => format!("send \"queries\":[{{…}}], not \"queries\":{value}"),
    }
}

/// For a string field: one value per query row, or the scalar quoted.
/// `row_field` says the field sits directly in a query row, where the other
/// values of an array belong in rows of their own.
pub(super) fn string(received: &Value, row_field: bool) -> String {
    match received {
        Value::Array(_) if row_field => {
            "Expected string, not an array; send one value, and each other value in its own query row"
                .to_owned()
        }
        Value::Array(_) => "Expected string, not an array".to_owned(),
        Value::Number(_) | Value::Bool(_) => {
            let text = received.to_string();
            format!("Expected string; send \"{text}\", not {text}")
        }
        _ => "Expected string".to_owned(),
    }
}

/// For an object field sent a scalar: every required property set to it,
/// when each takes a value of that type; a JSON string's object unquoted.
pub(super) fn object(schema: &Value, defs: &Value, received: &Value) -> Option<String> {
    if holds_json(received, Value::is_object) {
        return Some(SEND_ITSELF.to_owned());
    }
    let schema = super::super::resolve_ref(schema, defs);
    let required = schema.get("required")?.as_array()?;
    let properties = schema.get("properties")?.as_object()?;
    let mut fields = serde_json::Map::new();
    for name in required.iter().filter_map(Value::as_str) {
        let property = super::super::resolve_ref(properties.get(name)?, defs);
        let takes = match property.get("type").and_then(Value::as_str)? {
            "integer" => received
                .as_f64()
                .is_some_and(|number| number.fract() == 0.0),
            "number" => received.is_number(),
            "string" => received.is_string(),
            "boolean" => received.is_boolean(),
            _ => false,
        };
        if !takes {
            return None;
        }
        fields.insert(name.to_owned(), received.clone());
    }
    (!fields.is_empty()).then(|| format!("send {}, not {}", Value::Object(fields), shown(received)))
}

/// Lowercase without separators: `match_string`, `MatchString` and
/// `matchstring` all spell `matchstring`.
fn folded(name: &str) -> String {
    name.chars()
        .filter(|character| !matches!(character, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The one candidate `sent` most likely means: the same name in another
/// case or separator style, else the single nearest within two edits. A tie
/// or nothing near is no guess.
pub(super) fn closest<'a>(
    sent: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let sent = folded(sent);
    let mut best: Option<(usize, &str)> = None;
    let mut tied = false;
    for candidate in candidates {
        let distance = super::levenshtein(&sent, &folded(candidate));
        if distance > 2 || distance >= sent.chars().count() {
            continue;
        }
        match best {
            Some((nearest, _)) if distance > nearest => {}
            Some((nearest, current)) if distance == nearest => tied |= current != candidate,
            _ => {
                best = Some((distance, candidate));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(_, candidate)| candidate)
}
