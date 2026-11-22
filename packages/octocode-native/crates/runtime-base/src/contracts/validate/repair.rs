//! The repair half of a type or name error: the value the caller sent,
//! rendered in the shape the field takes, or the field or value it most
//! likely meant. Each is a clause appended to the error line, so one line
//! says what is wrong and what to send instead.
use serde_json::Value;

/// Widest received value a repair quotes; a longer one is elided.
const SHOWN_CHARS: usize = 60;

/// `value` as compact JSON, elided past [`SHOWN_CHARS`].
pub(super) fn shown(value: &Value) -> String {
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

/// The repaired `queries` value. A string holding the array and a lone
/// object are read before validation (`normalize::shaped`).
pub(crate) fn queries(received: &Value) -> String {
    format!(
        "send \"queries\":[{{…}}], not \"queries\":{}",
        shown(received)
    )
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

/// For an object field sent a JSON string holding its object: unquote it.
pub(super) fn object_text(received: &Value) -> Option<String> {
    holds_json(received, Value::is_object).then(|| SEND_ITSELF.to_owned())
}

/// The object form `schema` filled from the scalar `received`: a required
/// property pinned to a constant takes the constant, a scalar one takes
/// `received` when it is of that type, an object one is filled the same
/// way. `None` when a required property takes neither, or when none takes
/// `received` (constants alone would drop the value sent). Only the type is
/// matched here: the caller validates the candidate against the form.
pub(super) fn filled(schema: &Value, defs: &Value, received: &Value) -> Option<Value> {
    let mut placed = false;
    let candidate = fill_object(schema, defs, received, &mut placed, &mut Vec::new())?;
    placed.then_some(candidate)
}

/// `schema`'s required properties, each filled ([`fill`]). `open` holds the
/// object schemas being filled, so a schema requiring itself ends the fill.
fn fill_object<'a>(
    schema: &'a Value,
    defs: &'a Value,
    received: &Value,
    placed: &mut bool,
    open: &mut Vec<&'a Value>,
) -> Option<Value> {
    let schema = super::super::resolve_ref(schema, defs);
    if open.iter().any(|filling| std::ptr::eq(*filling, schema)) {
        return None;
    }
    let required = schema.get("required")?.as_array()?;
    let properties = schema.get("properties")?.as_object()?;
    open.push(schema);
    let mut fields = serde_json::Map::new();
    for name in required.iter().filter_map(Value::as_str) {
        let Some(value) = properties
            .get(name)
            .and_then(|property| fill(property, defs, received, placed, open))
        else {
            open.pop();
            return None;
        };
        fields.insert(name.to_owned(), value);
    }
    open.pop();
    (!fields.is_empty()).then_some(Value::Object(fields))
}

/// One required property's value: its constant, `received` when it is of
/// the property's scalar type, or its object filled.
fn fill<'a>(
    property: &'a Value,
    defs: &'a Value,
    received: &Value,
    placed: &mut bool,
    open: &mut Vec<&'a Value>,
) -> Option<Value> {
    let property = super::super::resolve_ref(property, defs);
    if let Some(constant) = property.get("const") {
        return Some(constant.clone());
    }
    let takes = match property.get("type").and_then(Value::as_str)? {
        "object" => return fill_object(property, defs, received, placed, open),
        "integer" => received
            .as_f64()
            .is_some_and(|number| number.fract() == 0.0),
        "number" => received.is_number(),
        "string" => received.is_string(),
        "boolean" => received.is_boolean(),
        _ => false,
    };
    *placed |= takes;
    takes.then(|| received.clone())
}

/// The repair for a scalar sent for an object: every object it completes
/// into a valid value ([`filled`], validated by the caller).
pub(super) fn send_instead(candidates: &[Value], received: &Value) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let objects = candidates
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    Some(format!("send {objects}, not {}", shown(received)))
}

/// A filter selection: `{include, exclude}` and nothing else. A row may declare
/// an `include` of its own without being one.
fn takes_selection(schema: &Value) -> bool {
    schema["properties"].as_object().is_some_and(|fields| {
        fields.contains_key("include")
            && fields
                .keys()
                .all(|field| field == "include" || field == "exclude")
    })
}

/// What is wrong with `received`, a bare list longer than a selection's
/// `include` takes: said of the list sent, which has no reading as an
/// `include` nobody wrote.
pub(super) fn selection_overflow(schema: &Value, defs: &Value, received: &Value) -> Option<String> {
    let schema = super::super::resolve_ref(schema, defs);
    let length = received.as_array()?.len();
    let include = super::super::resolve_ref(schema.pointer("/properties/include")?, defs);
    let maximum = include.get("maxItems")?.as_u64()?;
    (takes_selection(schema) && u64::try_from(length).is_ok_and(|length| length > maximum))
        .then(|| format!("Value length {length} exceeds the maximum of {maximum}"))
}

/// The value `received` is read as where `schema` declares another type,
/// when the schema alone gives it one typed reading: a text holding JSON of
/// the declared type is that JSON value, and a value where a list is
/// declared is the one-item list. Validation of the result decides whether
/// it fits.
pub(crate) fn lenient_value(schema: &Value, defs: &Value, received: &Value) -> Option<Value> {
    let schema = super::super::resolve_ref(schema, defs);
    let wanted: fn(&Value) -> bool = match schema.get("type").and_then(Value::as_str)? {
        "array" => Value::is_array,
        "object" => Value::is_object,
        "integer" | "number" => Value::is_number,
        "boolean" => Value::is_boolean,
        _ => return None,
    };
    if holds_json(received, wanted) {
        return serde_json::from_str(received.as_str()?).ok();
    }
    (wanted(&Value::Array(Vec::new())) && !received.is_array() && !received.is_null())
        .then(|| Value::Array(vec![received.clone()]))
}

/// The value an error's example shows for `received` where `schema` takes
/// another type: its lenient reading ([`lenient_value`]), else the fix the
/// error line names, which the caller makes: a selection's values as its
/// `include`, a scalar as the object it fills ([`filled`]), a scalar's
/// text, a one-item list's item.
pub(super) fn repaired_value(schema: &Value, defs: &Value, received: &Value) -> Option<Value> {
    if let Some(read) = lenient_value(schema, defs, received) {
        return Some(read);
    }
    let schema = super::super::resolve_ref(schema, defs);
    match schema.get("type").and_then(Value::as_str)? {
        "object" => {
            if selection_overflow(schema, defs, received).is_some() {
                return None;
            }
            if takes_selection(schema) && !received.is_object() {
                let values = if received.is_array() {
                    received.clone()
                } else {
                    Value::Array(vec![received.clone()])
                };
                return Some(serde_json::json!({"include": values}));
            }
            filled(schema, defs, received)
        }
        "string" => match received {
            Value::Number(_) | Value::Bool(_) => Some(Value::String(received.to_string())),
            Value::Array(items) => match items.as_slice() {
                [item @ Value::String(_)] => Some(item.clone()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Lowercase without separators: `match_string`, `MatchString` and
/// `matchstring` all spell `matchstring`.
fn folded(name: &str) -> String {
    name.chars()
        .filter(|character| !matches!(character, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether two names differ only in case or separator style.
pub(crate) fn same_name(left: &str, right: &str) -> bool {
    folded(left) == folded(right)
}

/// The literal of `schema`'s `enum` a text outside it means, when it has one
/// reading: the one literal it spells in another case or separator style
/// (`pypi` for `PyPI`). A former value is no reading: its error names what
/// it is now.
pub(crate) fn literal_reading(schema: &Value, received: &Value) -> Option<Value> {
    let sent = received.as_str()?;
    let mut spelled = schema
        .get("enum")?
        .as_array()?
        .iter()
        .filter(|literal| literal.as_str().is_some_and(|name| same_name(name, sent)));
    match (spelled.next(), spelled.next()) {
        (Some(only), None) => Some(only.clone()),
        _ => None,
    }
}

/// The candidates `sent` most likely means, each once in candidate order:
/// the same name in another case or separator style, else every one at the
/// nearest distance within two edits. Nothing near is none.
pub(super) fn nearest<'a>(
    sent: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let sent = folded(sent);
    let mut least: Option<usize> = None;
    let mut found = Vec::new();
    for candidate in candidates {
        let distance = super::levenshtein(&sent, &folded(candidate));
        if distance > 2 || distance >= sent.chars().count() {
            continue;
        }
        match least {
            Some(least) if distance > least => {}
            Some(least) if distance == least => {
                if !found.contains(&candidate) {
                    found.push(candidate);
                }
            }
            _ => {
                least = Some(distance);
                found = vec![candidate];
            }
        }
    }
    found
}

/// The words of a field name, lower-cased: `pullNumber` and `pull_number`
/// are `pull`, `number`.
fn words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut start = true;
    for character in name.chars() {
        if matches!(character, '_' | '-' | ' ' | '.') {
            start = true;
            continue;
        }
        if start || character.is_uppercase() || words.is_empty() {
            words.push(String::new());
        }
        start = false;
        if let Some(word) = words.last_mut() {
            word.extend(character.to_lowercase());
        }
    }
    words
}

/// The candidates whose name holds `sent`'s words in a row, or whose words
/// `sent` holds in a row (`context` for `contextLines`, `pullNumber` for
/// `number`), each once in candidate order: a name that is part of another
/// names the same thing more or less exactly. For a field no candidate is
/// within two edits of ([`nearest`]).
pub(super) fn sharing_words<'a>(
    sent: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let sent = words(sent);
    let holds = |whole: &[String], part: &[String]| {
        !part.is_empty() && whole.windows(part.len()).any(|window| window == part)
    };
    let mut found = Vec::new();
    for candidate in candidates {
        let named = words(candidate);
        if (holds(&named, &sent) || holds(&sent, &named)) && !found.contains(&candidate) {
            found.push(candidate);
        }
    }
    found
}

/// The one candidate `sent` most likely means ([`nearest`]); a tie or
/// nothing near is no guess.
pub(super) fn closest<'a>(
    sent: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    match nearest(sent, candidates).as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}
