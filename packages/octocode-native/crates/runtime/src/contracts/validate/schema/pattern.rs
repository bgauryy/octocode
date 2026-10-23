use super::super::{ValidationIssue, shape};
use regress::Regex;
use serde_json::Value;

/// Compiled schema patterns, keyed by source. Patterns come from the embedded
/// contract (a small fixed set), so the cache stays bounded. The ECMAScript
/// engine matches generated wire types; cache hits reuse the compiled pattern.
type PatternCache = std::collections::HashMap<String, Result<std::sync::Arc<Regex>, String>>;

pub(super) fn pattern_cache() -> &'static std::sync::Mutex<PatternCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<PatternCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub(super) fn compiled_pattern(pattern: &str) -> Result<std::sync::Arc<Regex>, String> {
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
pub(super) fn pattern_message(
    value: &str,
    path: &[String],
    pattern: &str,
    regex: &Regex,
    schema: &Value,
) -> String {
    if path.last().map(String::as_str) == Some("qualifiers")
        && let Some(term) = super::super::qualifiers::qualifier_terms(value)
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

/// A rejected line-range item: the value as sent, then the span it spells
/// when it spells one (`did you mean "3-6"?`), else the shape once
/// (`ranges: ["299-342"] (strings "start-end")`), named by its field.
pub(super) fn line_range_message(value: &Value, path: &[String]) -> String {
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
    match shape::line_range_reading(value) {
        Some(span) => format!("{sent} is not a line range (did you mean \"{span}\"?)"),
        None => {
            format!("{sent} is not a line range; {field}: [\"299-342\"] (strings \"start-end\")")
        }
    }
}

/// A pattern issue whose own schema has no note and a pattern too long to
/// read (`it must match ^(?:…`): the value and the enclosing field's `note`.
pub(super) fn describe_unreadable_pattern(issue: &mut ValidationIssue, note: &str) {
    let Some(schema) = issue.schema.as_ref() else {
        return;
    };
    let unreadable = issue.rule_id == "schema.pattern"
        && schema.get("description").is_none()
        && schema
            .get("pattern")
            .and_then(Value::as_str)
            .is_some_and(|pattern| pattern.chars().count() > READABLE_PATTERN_CHARS);
    if let (true, Some(text)) = (unreadable, issue.received.as_ref().and_then(Value::as_str))
        && !text.trim().is_empty()
    {
        let shown: String = text.chars().take(80).collect();
        issue.message = format!("\"{shown}\" does not match the expected form: {note}");
    }
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
