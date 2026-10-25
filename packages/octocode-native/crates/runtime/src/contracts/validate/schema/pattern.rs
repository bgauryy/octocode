use super::super::ValidationIssue;
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
    let compiled = Regex::with_flags(pattern, "u")
        .map(std::sync::Arc::new)
        .map_err(|error| error.to_string());
    cache.insert(pattern.to_owned(), compiled.clone());
    compiled
}

/// The longest pattern an error quotes as the expected form.
const READABLE_PATTERN_CHARS: usize = 40;

/// Explain blank values directly. Other values name the field and its pattern;
/// a long pattern uses the field's description when available.
pub(super) fn pattern_message(
    value: &str,
    path: &[String],
    pattern: &str,
    schema: &Value,
) -> String {
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
