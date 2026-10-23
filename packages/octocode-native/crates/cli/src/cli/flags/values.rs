use super::field_id;
use clap::ArgMatches;
use serde_json::{Value, json};

/// The value one flag holds in `matches`, as the field's JSON value.
pub(super) fn flag_value(
    matches: &ArgMatches,
    id: &str,
    kind: &str,
) -> Result<Option<Value>, String> {
    if !matches.contains_id(id)
        || matches.value_source(id) != Some(clap::parser::ValueSource::CommandLine)
    {
        return Ok(None);
    }
    Ok(Some(match kind {
        "bool" => json!(matches.get_one::<String>(id).map(String::as_str) != Some("false")),
        "integer" => json!(matches.get_one::<i64>(id).copied()),
        "integers" => json!(
            matches
                .get_many::<i64>(id)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        ),
        "number" => json!(matches.get_one::<f64>(id).copied()),
        "strings" => json!(
            matches
                .get_many::<String>(id)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        ),
        "json" => {
            let text = matches.get_one::<String>(id).cloned().unwrap_or_default();
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        }
        _ => json!(matches.get_one::<String>(id)),
    }))
}

/// Split `text` at `separator` into exactly two non-empty parts.
pub(super) fn split_two<'a>(
    text: &'a str,
    separator: &str,
    name: &str,
) -> Result<(&'a str, &'a str), String> {
    match text.split_once(separator) {
        Some((first, second)) if !first.is_empty() && !second.is_empty() => Ok((first, second)),
        _ => Err(format!("{name} must look like A{separator}B, got '{text}'")),
    }
}

/// A run of ASCII digits as a number.
pub(super) fn digits(text: &str) -> Option<u64> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// `PATH:N` or `PATH:N-M`. A local path that exists under the whole text
/// keeps it; a remote (GitHub) path is never checked on this disk.
/// A line span: its first line and, for a range, its last.
pub(super) type Span = (u64, Option<u64>);

pub(super) fn line_suffix(text: &str, local: bool) -> Option<(&str, Vec<Span>)> {
    if local && std::path::Path::new(text).exists() {
        return None;
    }
    let (path, lines) = text.rsplit_once(':')?;
    // `N`, `N-M`, or several joined by commas (`10-20,40-50`).
    let spans = lines
        .split(',')
        .map(|span| {
            let (start, end) = match span.split_once('-') {
                Some((start, end)) => (start, Some(digits(end)?)),
                None => (span, None),
            };
            Some((digits(start)?, end))
        })
        .collect::<Option<Vec<_>>>()?;
    (!path.is_empty()).then_some((path, spans))
}

/// Whether the caller set `field` by flag (or short letter).
pub(super) fn set_by_flag(matches: &ArgMatches, field: &str) -> bool {
    let id = field_id(field);
    matches.try_contains_id(&id).unwrap_or(false)
        && matches.value_source(&id) == Some(clap::parser::ValueSource::CommandLine)
}

/// A positional's text as the value its field takes.
pub(super) fn positional_value(spec: &Value, field: &str, text: &str) -> Value {
    let kind = spec["flags"]
        .as_array()
        .and_then(|flags| flags.iter().find(|flag| flag["field"] == field))
        .and_then(|flag| flag["kind"].as_str())
        .unwrap_or("string");
    match kind {
        "integer" => text
            .parse::<i64>()
            .map_or_else(|_| json!(text), |n| json!(n)),
        "strings" => json!([text]),
        _ => json!(text),
    }
}
