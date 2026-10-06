//! Lossless, schema-driven input repair applied before validation.
//!
//! Scalars: agents and shells often send `startLine:"10"` or
//! `fullContent:"true"`. A string is coerced only when every schema that can
//! hold the field types it `integer` or `boolean`; a field any branch leaves
//! untyped or accepts as a string is untouched. Integer: the string is exactly
//! the decimal rendering of a safe integer (JS: `Number.isSafeInteger(n) &&
//! String(n) === s`), so `"02"`, `"+2"`, `"-0"`, `"2.0"`, `"1e3"` and padded
//! values stay strings. Boolean: exactly `"true"` or `"false"`.
//!
//! Arrays: MCP hosts send list fields JSON-encoded (`include:"[\"*.go\"]"`)
//! or as a bare scalar (`keywords:"term"`). Where every schema that can hold
//! the value is an array (a `null` alternative is neutral), a string that
//! parses as a JSON array (curly double quotes read as `"`), or as that JSON
//! encoded once more (`[\"a\"]`), becomes that array, and a scalar every
//! array alternative's items accept becomes a one-element array. Any string,
//! untyped, or other alternative vetoes the repair.
//!
//! Line ranges: where items carry the canonical `a-b` line-range pattern,
//! `" 140-150"`, `"140 - 150"` and `"70,130"` become `"140-150"`/`"70-130"`,
//! a bare line `"190"` becomes `"190-190"`, and a pair of bare line numbers
//! (`["248","325"]`, `[248,325]`, or one pair per item: `[["248","325"]]`)
//! becomes the one range holding both. Every requested line stays in the
//! read.
//!
//! Closed string sets: where every alternative is a string `enum`/`const`, a
//! value that names exactly one member up to case, `-`/`_`/space, or a plural
//! `s`/`es` (`pullRequests`, `PullRequest`, `patch`) becomes that member.

use serde_json::Value;

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
/// The canonical line-range item pattern (`95-105`).
const LINE_RANGE_PATTERN: &str = r"^[1-9]\d*-[1-9]\d*$";
const MAX_REF_DEPTH: usize = 32;

/// A schema and the root its `$ref`s resolve against.
type Typed<'a> = (&'a Value, &'a Value);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Integer,
    Boolean,
}

pub(super) fn coerce_lossless(candidates: &[Typed<'_>], value: &mut Value) {
    let schemas = flatten_all(candidates);
    if schemas.is_empty() {
        return;
    }
    if let Some(array) = array_form(&schemas, value) {
        *value = array;
    }
    match value {
        Value::String(text) => {
            if line_range_only(&schemas) {
                if let Some(range) = line_range(text) {
                    *value = Value::String(range);
                }
            } else if let Some(coerced) = agreed_scalar(&schemas).and_then(|kind| parse(kind, text))
            {
                *value = coerced;
            } else if let Some(member) =
                closed_strings(&schemas).and_then(|members| near_miss(&members, text))
            {
                *value = Value::String(member.to_owned());
            }
        }
        Value::Object(object) => {
            for (key, field) in object.iter_mut() {
                let children = schemas
                    .iter()
                    .filter_map(|&(root, schema)| {
                        schema
                            .get("properties")?
                            .get(key)
                            .map(|child| (root, child))
                    })
                    .collect::<Vec<_>>();
                coerce_lossless(&children, field);
            }
        }
        Value::Array(items) => {
            let children = item_schemas(&schemas);
            let ranges = line_range_only(&flatten_all(&children));
            if ranges && let Some(range) = line_pair(items) {
                *items = vec![Value::String(range)];
                return;
            }
            for item in items {
                if ranges
                    && let Value::Array(pair) = item
                    && let Some(range) = line_pair(pair)
                {
                    *item = Value::String(range);
                } else {
                    coerce_lossless(&children, item);
                }
            }
        }
        _ => {}
    }
}

/// Drops a row's blank optional values: an empty or whitespace-only string,
/// or an empty list, in a field no variant requires and every alternative
/// rejects as blank. Such a value carries nothing, so it means the field was
/// omitted; a required field keeps its value for the validator to name.
pub(super) fn drop_blank_optionals(candidates: &[Typed<'_>], row: &mut Value) {
    let variants = flatten_all(candidates);
    let required: Vec<&str> = variants
        .iter()
        .filter_map(|(_, schema)| schema.get("required")?.as_array())
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let Value::Object(object) = row else {
        return;
    };
    object.retain(|key, value| {
        if required.contains(&key.as_str()) || !is_blank(value) {
            return true;
        }
        let fields = variants
            .iter()
            .filter_map(|&(root, schema)| {
                schema
                    .get("properties")?
                    .get(key)
                    .map(|field| (root, field))
            })
            .collect::<Vec<_>>();
        let alternatives = flatten_all(&fields);
        alternatives.is_empty()
            || !alternatives
                .iter()
                .all(|(_, schema)| rejects_blank(schema, value))
    });
}

fn is_blank(value: &Value) -> bool {
    match value {
        Value::String(text) => text.trim().is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

/// Whether a flattened schema rejects this blank string or empty list.
fn rejects_blank(schema: &Value, value: &Value) -> bool {
    let at_least = |key: &str| {
        schema
            .get(key)
            .and_then(Value::as_u64)
            .is_some_and(|n| n >= 1)
    };
    match value {
        Value::String(text) => {
            types(schema).is_some_and(|names| names == ["string"])
                && ((text.is_empty() && at_least("minLength"))
                    || schema.get("pattern").and_then(Value::as_str) == Some(r"\S"))
        }
        Value::Array(_) => {
            types(schema).is_some_and(|names| names == ["array"]) && at_least("minItems")
        }
        _ => false,
    }
}

/// Whether `schema` (an array schema at any `$ref`/union depth) has items
/// that accept `value`, e.g. to decide if wrapping a scalar is sound advice.
pub(super) fn items_accept(root: &Value, schema: &Value, value: &Value) -> bool {
    let schemas = flatten_all(&[(root, schema)]);
    let arrays = schemas
        .iter()
        .filter(|(_, schema)| types(schema).is_some_and(|names| names.contains(&"array")))
        .collect::<Vec<_>>();
    !arrays.is_empty()
        && arrays.iter().all(|&&(root, schema)| {
            schema.get("items").is_none_or(|items| {
                flatten_all(&[(root, items)])
                    .iter()
                    .any(|(_, item)| accepts(item, value))
            })
        })
}

/// The array a list-only position means by a JSON-encoded array string or a
/// bare scalar; `None` when the value is not repaired.
fn array_form(schemas: &[Typed<'_>], value: &Value) -> Option<Value> {
    if matches!(value, Value::Null | Value::Array(_) | Value::Object(_)) || !only_arrays(schemas) {
        return None;
    }
    if let Value::String(text) = value {
        if let Some(parsed) = encoded_array(text) {
            return Some(parsed);
        }
        // A malformed encoded list is not one literal item; leave it for the
        // validator to report. Glob classes such as `[ab]*.ts` still wrap.
        if looks_like_json_array(text) {
            return None;
        }
    }
    let mut item = value.clone();
    coerce_lossless(&item_schemas(schemas), &mut item);
    schemas
        .iter()
        .all(|&(root, schema)| {
            types(schema).is_some_and(|names| names == ["null"])
                || items_accept(root, schema, &item)
        })
        .then(|| Value::Array(vec![item]))
}

/// The array `text` encodes: JSON (curly double quotes read as `"`), or that
/// JSON encoded once more, with its quotes escaped (`[\"a\"]`) or as a JSON
/// string (`"[\"a\"]"`).
fn encoded_array(text: &str) -> Option<Value> {
    let parse = |text: &str| match serde_json::from_str::<Value>(text) {
        Ok(parsed @ Value::Array(_)) => Some(parsed),
        _ => None,
    };
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        return parse(trimmed)
            .or_else(|| parse(&straight_quotes(trimmed)))
            .or_else(|| {
                let unescaped = serde_json::from_str::<String>(&format!("\"{trimmed}\"")).ok()?;
                parse(&unescaped)
            });
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::String(inner)) if inner.trim_start().starts_with('[') => parse(inner.trim()),
        _ => None,
    }
}

/// `["…`, `[{…`, `[[…` or `[\"…`: the text is an attempted JSON array, not a
/// literal. A curly double quote counts as `"`.
pub(super) fn looks_like_json_array(text: &str) -> bool {
    let mut chars = text.trim_start().chars();
    chars.next() == Some('[')
        && chars.find(|c| !c.is_whitespace()).is_some_and(|c| {
            matches!(c, '"' | '{' | '[' | '\\') || CURLY_DOUBLE_QUOTES.contains(&c)
        })
}

/// Double quotes rich-text hosts substitute for `"`.
const CURLY_DOUBLE_QUOTES: [char; 5] = ['\u{201c}', '\u{201d}', '\u{201e}', '\u{201f}', '\u{2033}'];

/// `text` with every curly double quote replaced by `"`: the JSON a
/// rich-text host meant when it typographically quoted an encoded list.
fn straight_quotes(text: &str) -> String {
    text.chars()
        .map(|c| {
            if CURLY_DOUBLE_QUOTES.contains(&c) {
                '"'
            } else {
                c
            }
        })
        .collect()
}

/// Every alternative that can hold the value is an array; `null` is neutral.
fn only_arrays(schemas: &[Typed<'_>]) -> bool {
    let mut array = false;
    for (_, schema) in schemas {
        let Some(names) = types(schema) else {
            return false;
        };
        for name in names {
            match name {
                "null" => {}
                "array" => array = true,
                _ => return false,
            }
        }
    }
    array
}

/// Whether a flattened item schema's declared type admits `value`; an untyped
/// schema admits anything.
fn accepts(schema: &Value, value: &Value) -> bool {
    types(schema).is_none_or(|names| {
        names.iter().any(|name| match *name {
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "null" => value.is_null(),
            _ => false,
        })
    })
}

fn types(schema: &Value) -> Option<Vec<&str>> {
    match schema.get("type") {
        Some(Value::String(name)) => Some(vec![name.as_str()]),
        Some(Value::Array(names)) => Some(names.iter().filter_map(Value::as_str).collect()),
        _ => None,
    }
}

fn item_schemas<'a>(schemas: &[Typed<'a>]) -> Vec<Typed<'a>> {
    schemas
        .iter()
        .filter_map(|&(root, schema)| schema.get("items").map(|child| (root, child)))
        .collect()
}

fn flatten_all<'a>(candidates: &[Typed<'a>]) -> Vec<Typed<'a>> {
    let mut schemas = Vec::new();
    for &(root, schema) in candidates {
        flatten(root, schema, 0, &mut schemas);
    }
    schemas
}

/// Resolve `$ref`s and expand union branches into the concrete schemas a
/// value may be checked against. `{"not":{}}` forbids the field in that
/// branch, so it cannot hold the value and is dropped.
fn flatten<'a>(root: &'a Value, schema: &'a Value, depth: usize, out: &mut Vec<Typed<'a>>) {
    if depth > MAX_REF_DEPTH {
        out.push((root, &Value::Null));
        return;
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = root
            .pointer(reference.strip_prefix('#').unwrap_or(reference))
            .unwrap_or(&Value::Null);
        return flatten(root, target, depth + 1, out);
    }
    let branches = ["oneOf", "anyOf", "allOf"]
        .iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .collect::<Vec<_>>();
    if !branches.is_empty() {
        for branch in branches {
            flatten(root, branch, depth + 1, out);
        }
        return;
    }
    let forbidden = schema.as_object().is_some_and(|object| {
        object.len() == 1
            && object
                .get("not")
                .is_some_and(|not| not == &Value::Object(Default::default()))
    });
    if !forbidden {
        out.push((root, schema));
    }
}

/// Every alternative is a string holding one canonical line range.
fn line_range_only(schemas: &[Typed<'_>]) -> bool {
    !schemas.is_empty()
        && schemas.iter().all(|(_, schema)| {
            types(schema).is_some_and(|names| names == ["string"])
                && schema.get("pattern").and_then(Value::as_str) == Some(LINE_RANGE_PATTERN)
        })
}

/// A positive line number sent bare: an integer or its decimal string.
fn line_number(value: &Value) -> Option<u64> {
    let number = match value {
        Value::Number(number) => number.as_u64()?,
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            text.parse().ok()?
        }
        _ => return None,
    };
    (number > 0 && number <= MAX_SAFE_INTEGER).then_some(number)
}

/// Two bare line numbers `[a, b]` with `a <= b`, as the range `a-b`.
fn line_pair(items: &[Value]) -> Option<String> {
    let [first, second] = items else {
        return None;
    };
    let (start, end) = (line_number(first)?, line_number(second)?);
    (start <= end).then(|| format!("{start}-{end}"))
}

/// The members of a closed string set: every alternative is a string `enum`
/// or `const` (a `null` alternative is neutral); `None` when any alternative
/// is open.
fn closed_strings<'a>(schemas: &[Typed<'a>]) -> Option<Vec<&'a str>> {
    let mut members = Vec::new();
    for (_, schema) in schemas {
        if types(schema).is_some_and(|names| names == ["null"]) {
            continue;
        }
        if let Some(constant) = schema.get("const") {
            members.push(constant.as_str()?);
            continue;
        }
        for member in schema.get("enum")?.as_array()? {
            members.push(member.as_str()?);
        }
    }
    (!members.is_empty()).then_some(members)
}

/// The one member `text` means up to case and `-`/`_`/space separators, else
/// up to a plural `s`/`es` on either side; `None` for an exact member, no
/// match, or more than one candidate.
fn near_miss<'a>(members: &[&'a str], text: &str) -> Option<&'a str> {
    if members.contains(&text) {
        return None;
    }
    let key = |value: &str| -> String {
        value
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .flat_map(char::to_lowercase)
            .collect()
    };
    let wanted = key(text);
    let plural = |singular: &str, other: &str| {
        other
            .strip_prefix(singular)
            .is_some_and(|suffix| matches!(suffix, "s" | "es"))
    };
    let unique = |candidates: Vec<&'a str>| {
        let first = *candidates.first()?;
        candidates
            .iter()
            .all(|member| *member == first)
            .then_some(first)
    };
    let exact: Vec<&str> = members
        .iter()
        .copied()
        .filter(|member| key(member) == wanted)
        .collect();
    if !exact.is_empty() {
        return unique(exact);
    }
    unique(
        members
            .iter()
            .copied()
            .filter(|member| {
                let member = key(member);
                plural(&member, &wanted) || plural(&wanted, &member)
            })
            .collect(),
    )
}

/// `a-b` or `a,b` with optional spaces around either number, as `a-b`, and a
/// bare line `a` as the one-line range `a-a`; `None` for any other text (left
/// for the validator to report).
fn line_range(text: &str) -> Option<String> {
    let Some((start, end)) = text.split_once(['-', ',']) else {
        let line = line_number(&Value::String(text.to_owned()))?;
        return Some(format!("{line}-{line}"));
    };
    let start = line_number(&Value::String(start.to_owned()))?;
    let end = line_number(&Value::String(end.to_owned()))?;
    Some(format!("{start}-{end}"))
}

/// The one scalar type every alternative agrees on; a `null` alternative is
/// neutral, any other type (string, untyped) vetoes coercion.
fn agreed_scalar(schemas: &[Typed<'_>]) -> Option<Scalar> {
    let mut agreed = None;
    for (_, schema) in schemas {
        for name in types(schema)? {
            let kind = match name {
                "null" => continue,
                "integer" => Scalar::Integer,
                "boolean" => Scalar::Boolean,
                _ => return None,
            };
            if agreed
                .replace(kind)
                .is_some_and(|previous| previous != kind)
            {
                return None;
            }
        }
    }
    agreed
}

fn parse(kind: Scalar, text: &str) -> Option<Value> {
    match kind {
        Scalar::Integer => {
            let number = text.parse::<i64>().ok()?;
            (number.unsigned_abs() <= MAX_SAFE_INTEGER && number.to_string() == text)
                .then(|| Value::from(number))
        }
        Scalar::Boolean => match text {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::coerce_lossless;
    use serde_json::{Value, json};

    fn coerced(schema: &Value, value: Value) -> Value {
        let mut value = json!({"field": value});
        let object = json!({"type":"object","properties":{"field":schema}});
        coerce_lossless(&[(&object, &object)], &mut value);
        value["field"].take()
    }

    #[test]
    fn nullable_alternatives_coerce_and_string_alternatives_veto() {
        let nullable = json!({"anyOf":[{"type":"integer"},{"type":"null"}]});
        assert_eq!(coerced(&nullable, json!("3")), json!(3));
        assert_eq!(
            coerced(&json!({"type":["boolean","null"]}), json!("true")),
            json!(true)
        );
        let widened = json!({"anyOf":[{"type":"integer"},{"type":"string"}]});
        assert_eq!(coerced(&widened, json!("3")), json!("3"));
        assert_eq!(coerced(&json!({}), json!("3")), json!("3"));
        let mixed = json!({"anyOf":[{"type":"integer"},{"type":"boolean"}]});
        assert_eq!(coerced(&mixed, json!("true")), json!("true"));
    }

    #[test]
    fn only_canonical_decimal_safe_integers_coerce() {
        let integer = json!({"type":"integer"});
        for (text, expected) in [
            ("0", json!(0)),
            ("-7", json!(-7)),
            ("9007199254740991", json!(9_007_199_254_740_991_i64)),
        ] {
            assert_eq!(coerced(&integer, json!(text)), expected, "{text}");
        }
        for text in [
            " 3",
            "3.0",
            "1e2",
            "+3",
            "03",
            "-0",
            "0x10",
            "9007199254740992",
            "",
        ] {
            assert_eq!(coerced(&integer, json!(text)), json!(text), "{text}");
        }
    }

    #[test]
    fn json_encoded_arrays_parse_where_only_arrays_are_accepted() {
        let strings = json!({"type":"array","items":{"type":"string"}});
        assert_eq!(
            coerced(&strings, json!(r#"["*.go","*.rs"]"#)),
            json!(["*.go", "*.rs"])
        );
        assert_eq!(coerced(&strings, json!(r#" [ "a" ] "#)), json!(["a"]));
        assert_eq!(coerced(&strings, json!("[]")), json!([]));
        // Curly quotes from a rich-text host delimit the items.
        assert_eq!(
            coerced(
                &strings,
                json!("[\u{201c}*.go\u{201d}, \u{201c}*.rs\u{201d}]")
            ),
            json!(["*.go", "*.rs"])
        );
        // A malformed curly encoding is not one literal item.
        assert_eq!(
            coerced(&strings, json!("[\u{201c}*.go\u{201d}")),
            json!("[\u{201c}*.go\u{201d}")
        );
        let integers =
            json!({"anyOf":[{"type":"array","items":{"type":"integer"}},{"type":"null"}]});
        assert_eq!(coerced(&integers, json!(r#"["3",4]"#)), json!([3, 4]));
        let nested = json!({"type":"object","properties":{"include":strings}});
        assert_eq!(
            coerced(&nested, json!({"include": r#"["*.ts"]"#})),
            json!({"include":["*.ts"]})
        );
    }

    /// A list encoded one level too deep (escaped quotes, or the encoded
    /// list itself JSON-quoted) is still that list.
    #[test]
    fn double_encoded_arrays_parse_where_only_arrays_are_accepted() {
        let ranges =
            json!({"type":"array","items":{"type":"string","pattern":"^[1-9]\\d*-[1-9]\\d*$"}});
        assert_eq!(
            coerced(&ranges, json!(r#"[\"749-775\"]"#)),
            json!(["749-775"])
        );
        assert_eq!(
            coerced(&ranges, json!(r#""[\"10-20\",\"30-40\"]""#)),
            json!(["10-20", "30-40"])
        );
        // A malformed escaped list stays for the validator to report.
        assert_eq!(
            coerced(&ranges, json!(r#"[\"749-775\""#)),
            json!(r#"[\"749-775\""#)
        );
    }

    #[test]
    fn bare_scalars_wrap_when_every_array_alternative_accepts_them() {
        let strings = json!({"type":"array","items":{"type":"string"}});
        assert_eq!(coerced(&strings, json!("term")), json!(["term"]));
        assert_eq!(coerced(&strings, json!("[ab]*.ts")), json!(["[ab]*.ts"]));
        let integers = json!({"type":"array","items":{"type":"integer"}});
        assert_eq!(coerced(&integers, json!(5)), json!([5]));
        assert_eq!(coerced(&integers, json!("5")), json!([5]));
        assert_eq!(coerced(&integers, json!("five")), json!("five"));
        assert_eq!(coerced(&integers, json!(true)), json!(true));
        let untyped = json!({"type":"array"});
        assert_eq!(coerced(&untyped, json!(false)), json!([false]));
        let referenced = json!({"$ref":"#/properties/field/$defs/list","$defs":{"list":strings}});
        assert_eq!(coerced(&referenced, json!("x")), json!(["x"]));
    }

    /// Host spellings of a line range become the canonical `a-b`; a pair
    /// becomes the one range that holds both lines, so no requested line is
    /// lost. Anything else is left for the validator to report.
    #[test]
    fn line_range_spellings_repair_to_canonical_ranges() {
        let ranges =
            json!({"type":"array","items":{"type":"string","pattern":"^[1-9]\\d*-[1-9]\\d*$"}});
        for (input, expected) in [
            (json!("70,130"), json!(["70-130"])),
            (json!(" 140-150"), json!(["140-150"])),
            (json!(["140 - 150", "9-9"]), json!(["140-150", "9-9"])),
            (json!(["248", "325"]), json!(["248-325"])),
            (json!([248, 325]), json!(["248-325"])),
            (json!("[248,325]"), json!(["248-325"])),
            (json!(["70,130", " 1-2 "]), json!(["70-130", "1-2"])),
            (json!(["95-105"]), json!(["95-105"])),
            (json!([["248", "325"]]), json!(["248-325"])),
            (json!([[1, 2], ["5", 9]]), json!(["1-2", "5-9"])),
            (json!([[3, 4], "10,20"]), json!(["3-4", "10-20"])),
            (json!("[[248,325]]"), json!(["248-325"])),
            (
                json!(["1084", "1084-1140"]),
                json!(["1084-1084", "1084-1140"]),
            ),
            (json!(["190", "236-310"]), json!(["190-190", "236-310"])),
            (json!(["190"]), json!(["190-190"])),
            (json!("190"), json!(["190-190"])),
            (json!("[\u{201c}63-120\u{201d}]"), json!(["63-120"])),
        ] {
            assert_eq!(coerced(&ranges, input.clone()), expected, "{input}");
        }
        for input in [
            json!(["3:5"]),
            json!(["0-5"]),
            json!([325, 248]),
            json!(["a-b"]),
            json!([1, 2, 3]),
            json!(["1-2-3"]),
            json!([[325, 248]]),
            json!([[1, 2, 3]]),
            json!([[1]]),
            json!([["a", "b"]]),
        ] {
            assert_eq!(coerced(&ranges, input.clone()), input, "{input}");
        }
        // Only the line-range pattern is repaired.
        let plain = json!({"type":"array","items":{"type":"string"}});
        assert_eq!(coerced(&plain, json!(["70,130"])), json!(["70,130"]));
    }

    /// A closed string set repairs a value that matches exactly one member
    /// up to case, `-`/`_`/space separators, or a plural `s`/`es`; an exact
    /// match is preferred over a plural one, and anything ambiguous or open
    /// is left for the validator to report.
    #[test]
    fn near_miss_enum_values_repair_only_when_unambiguous() {
        let operation = json!({"type":"string","enum":["pullRequest","issue","commit"]});
        for (input, expected) in [
            ("pullRequests", "pullRequest"),
            ("PullRequest", "pullRequest"),
            ("pull_request", "pullRequest"),
            ("issues", "issue"),
            ("commits", "commit"),
            ("issue", "issue"),
        ] {
            assert_eq!(
                coerced(&operation, json!(input)),
                json!(expected),
                "{input}"
            );
        }
        for input in ["pr", "pulls", "", "issuess"] {
            assert_eq!(coerced(&operation, json!(input)), json!(input), "{input}");
        }
        let patches = json!({"type":"array","items":{"type":"string","enum":["body","patches"]}});
        assert_eq!(
            coerced(&patches, json!(["patch", "Body"])),
            json!(["patches", "body"])
        );
        // Discriminated variants contribute their `const` values.
        let union = json!({"anyOf":[
            {"type":"string","const":"definition"},
            {"type":"string","enum":["references","documentSymbols"]}
        ]});
        assert_eq!(coerced(&union, json!("reference")), json!("references"));
        assert_eq!(coerced(&union, json!("definitions")), json!("definition"));
        // Exact (case-insensitive) beats plural; two plural candidates veto.
        let both = json!({"type":"string","enum":["file","files"]});
        assert_eq!(coerced(&both, json!("FILES")), json!("files"));
        let ambiguous = json!({"type":"string","enum":["base","bas"]});
        assert_eq!(coerced(&ambiguous, json!("bases")), json!("bases"));
        // An open string alternative vetoes repair.
        let open = json!({"anyOf":[{"type":"string","enum":["a"]},{"type":"string"}]});
        assert_eq!(coerced(&open, json!("as")), json!("as"));
    }

    /// A blank value in an optional field every alternative rejects as blank
    /// (`pattern:"\\S"`, `minLength`, `minItems`) carries nothing, so it is
    /// dropped; required fields and values a schema accepts stay.
    #[test]
    fn blank_optional_values_drop_and_required_ones_stay() {
        let row = json!({
            "type":"object",
            "required":["path"],
            "properties":{
                "path":{"type":"string","pattern":"\\S"},
                "branch":{"type":"string","pattern":"\\S"},
                "ref":{"type":"string","minLength":1},
                "keywords":{"type":"array","minItems":1,"items":{"type":"string"}},
                "include":{"type":"array","items":{"type":"string"}},
                "note":{"type":"string"},
                "label":{"anyOf":[{"type":"string","minLength":1},{"type":"string"}]}
            }
        });
        let mut value = json!({
            "path":"  ","branch":" ","ref":"","keywords":[],"include":[],
            "note":"","label":"","other":""
        });
        super::drop_blank_optionals(&[(&row, &row)], &mut value);
        assert_eq!(
            value,
            json!({"path":"  ","include":[],"note":"","label":"","other":""})
        );
        let mut spaced = json!({"ref":" "});
        super::drop_blank_optionals(&[(&row, &row)], &mut spaced);
        assert_eq!(spaced, json!({"ref":" "}), "minLength accepts whitespace");
        // A field one variant requires stays in every row.
        let union = json!({"anyOf":[
            {"type":"object","required":["ref"],"properties":{"ref":{"type":"string","minLength":1}}},
            {"type":"object","properties":{"ref":{"type":"string","minLength":1}}}
        ]});
        let mut kept = json!({"ref":""});
        super::drop_blank_optionals(&[(&union, &union)], &mut kept);
        assert_eq!(kept, json!({"ref":""}));
    }

    #[test]
    fn string_untyped_and_non_array_alternatives_veto_array_repair() {
        let widened =
            json!({"anyOf":[{"type":"array","items":{"type":"string"}},{"type":"string"}]});
        assert_eq!(coerced(&widened, json!("term")), json!("term"));
        assert_eq!(coerced(&widened, json!(r#"["a"]"#)), json!(r#"["a"]"#));
        let open = json!({"anyOf":[{"type":"array"},{}]});
        assert_eq!(coerced(&open, json!("term")), json!("term"));
        let objects = json!({"type":"array","items":{"type":"object"}});
        assert_eq!(coerced(&objects, json!("term")), json!("term"));
        let strings = json!({"type":"array","items":{"type":"string"}});
        for text in [r#"["a""#, r#"[{"a":1}"#, r#"{"a":1}"#] {
            let expected = if text.starts_with('{') {
                json!([text])
            } else {
                json!(text)
            };
            assert_eq!(coerced(&strings, json!(text)), expected, "{text}");
        }
        assert_eq!(coerced(&strings, Value::Null), Value::Null);
        assert_eq!(coerced(&strings, json!({"a":1})), json!({"a":1}));
        assert_eq!(
            coerced(&json!({"type":"string"}), json!(r#"["a"]"#)),
            json!(r#"["a"]"#)
        );
    }
}
