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

/// What a value of a JSON Schema `format` must be (2020-12 Validation
/// §7.3: the RFC 3339 forms with an example), else the format's name.
fn format_requirement(format: &str) -> String {
    match format {
        "date-time" => "a date-time (RFC 3339), e.g. 2026-10-01T00:00:00Z".into(),
        "date" => "a full-date (RFC 3339), e.g. 2026-10-01".into(),
        "time" => "a full-time (RFC 3339), e.g. 00:00:00Z".into(),
        other => format!("in the {other} format"),
    }
}

/// The rule of a repository path, on every field that takes one.
const REPOSITORY_PATH_RULE: &str =
    "it must be repository-relative, with forward slashes and no \".\" or \"..\" segments";

/// Whether `pattern` behaves as a repository path: it takes a nested relative
/// path and refuses an absolute one, one that climbs out, and a backslash. Read
/// from what it does, since the pattern itself is too long to say so.
fn is_repository_path(pattern: &str) -> bool {
    compiled_pattern(pattern).is_ok_and(|compiled| {
        compiled.find("a/b").is_some()
            && ["/a", "../a", "a/../b", "a/./b", "a\\b"]
                .iter()
                .all(|path| compiled.find(path).is_none())
    })
}

/// The rule the contract states for a pattern no words can be read from: the
/// message of its numeric tokens, or a repository path's.
fn stated_rule(pattern: &str, schema: &Value) -> Option<String> {
    if let Some(message) = schema["x-numericTokens"]["message"].as_str() {
        return Some(message.to_owned());
    }
    is_repository_path(pattern).then(|| REPOSITORY_PATH_RULE.to_owned())
}

/// Explain blank values directly. Other values name the field and what it
/// must be, in words, since no published schema shows a pattern: its declared
/// `format`; a rule the contract states ([`stated_rule`]); for a pattern too
/// long to read, the field's note; what
/// [`plain_rule`] can say of the pattern; else the note, or the field alone.
pub(super) fn pattern_message(
    value: &str,
    path: &[String],
    pattern: &str,
    schema: &Value,
) -> String {
    if value.trim().is_empty() {
        return "is empty; give non-blank text.".into();
    }
    let shown: String = value.chars().take(80).collect();
    let invalid = match path
        .iter()
        .rev()
        .find(|segment| segment.parse::<usize>().is_err())
    {
        Some(field) => format!("\"{shown}\" is not a valid value for {field}"),
        None => format!("\"{shown}\" is not valid"),
    };
    if let Some(format) = schema.get("format").and_then(Value::as_str) {
        return format!("{invalid}: it must be {}", format_requirement(format));
    }
    if let Some(rule) = stated_rule(pattern, schema) {
        return format!("{invalid}: {rule}");
    }
    let note = schema.get("description").and_then(Value::as_str);
    let readable = pattern.chars().count() <= READABLE_PATTERN_CHARS;
    match (note, plain_rule(pattern)) {
        (Some(note), Some(_)) if !readable => {
            format!("\"{shown}\" does not match the expected form: {note}")
        }
        (_, Some(rule)) => format!("{invalid}: {rule}"),
        (Some(note), None) => format!("\"{shown}\" does not match the expected form: {note}"),
        (None, None) => invalid,
    }
}

/// One character-class atom of a pattern, in words.
enum Atom {
    Holds(Vec<String>),
    Refuses(Vec<String>),
}

/// The words for a character escape; `None` for one no word names (`\\p`).
fn escape_words(escaped: char) -> Option<Vec<String>> {
    Some(match escaped {
        'w' => vec!["letters".into(), "digits".into(), "'_'".into()],
        'd' => vec!["digits".into()],
        's' => vec!["whitespace".into()],
        'r' | 'n' => vec!["line breaks".into()],
        '0' => vec!["NUL".into()],
        'p' | 'P' | 'b' | 'B' | 'S' | 'W' | 'D' => return None,
        other => vec![format!("'{other}'")],
    })
}

/// The class after its `[`, through its `]`.
fn class_atom(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<Atom> {
    let negated = chars.next_if_eq(&'^').is_some();
    let mut words = Vec::new();
    loop {
        match chars.next()? {
            ']' => break,
            '\\' => words.extend(escape_words(chars.next()?)?),
            low => {
                let mut ahead = chars.clone();
                if ahead.next() == Some('-')
                    && let Some(high) = ahead.next().filter(|high| *high != ']')
                {
                    *chars = ahead;
                    words.push(match (low, high) {
                        ('a', 'z') | ('A', 'Z') => "letters".to_owned(),
                        ('0', '9') => "digits".to_owned(),
                        _ => format!("{low}-{high}"),
                    });
                } else {
                    words.push(format!("'{low}'"));
                }
            }
        }
    }
    Some(if negated {
        Atom::Refuses(words)
    } else {
        Atom::Holds(words)
    })
}

/// Adds `words` to `list`, each once, in first-seen order.
fn extend_unique(list: &mut Vec<String>, words: &[String]) {
    for word in words {
        if !list.contains(word) {
            list.push(word.clone());
        }
    }
}

/// The rule a pattern built from character classes states, in words: what a
/// value may hold or must not hold, how it starts and how long it is. `None`
/// for any other pattern (keywords, lookaround, classes that vary by position
/// around a literal), whose words would mislead.
fn plain_rule(pattern: &str) -> Option<String> {
    let alternatives = pattern.contains('|');
    let mut chars = pattern.chars().peekable();
    let (mut holds, mut refuses, mut starts) = (Vec::new(), Vec::new(), Vec::new());
    let mut prefix = String::new();
    let (mut at_start, mut grouped, mut seen_atom) = (true, false, false);
    let mut length = None;
    while let Some(next) = chars.next() {
        let atom = match next {
            '^' | '$' | ')' | '*' | '+' | '?' => continue,
            '|' => {
                at_start = true;
                continue;
            }
            '(' => {
                grouped = true;
                if chars.next_if_eq(&'?').is_some() && chars.next()? != ':' {
                    return None;
                }
                continue;
            }
            '{' => {
                let mut counts = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        digit => counts.push(digit),
                    }
                }
                length = Some(counts);
                continue;
            }
            '[' => class_atom(&mut chars)?,
            '\\' => match chars.next()? {
                'S' => Atom::Refuses(vec!["whitespace".into()]),
                escaped if escaped.is_ascii_alphanumeric() => Atom::Holds(escape_words(escaped)?),
                literal if !seen_atom && !alternatives => {
                    prefix.push(literal);
                    continue;
                }
                literal if alternatives => Atom::Holds(vec![format!("'{literal}'")]),
                _ => return None,
            },
            literal if !seen_atom && !alternatives => {
                prefix.push(literal);
                continue;
            }
            literal if alternatives && !literal.is_alphanumeric() => {
                Atom::Holds(vec![format!("'{literal}'")])
            }
            _ => return None,
        };
        seen_atom = true;
        match atom {
            Atom::Holds(words) => {
                if at_start {
                    extend_unique(&mut starts, &words);
                }
                extend_unique(&mut holds, &words);
            }
            Atom::Refuses(words) => extend_unique(&mut refuses, &words),
        }
        at_start = false;
    }
    let lead = if prefix.is_empty() {
        String::new()
    } else {
        format!("it must start with {prefix} and ")
    };
    let subject = if prefix.is_empty() { "it " } else { "" };
    if !refuses.is_empty() {
        return holds
            .is_empty()
            .then(|| format!("{lead}{subject}must not contain {}", refuses.join(", ")));
    }
    if holds.is_empty() {
        return None;
    }
    let held = holds.join(", ");
    if let Some(counts) = length.filter(|_| !grouped) {
        return Some(format!(
            "{lead}{subject}must be {} characters from: {held}",
            counts.replace(',', " to ")
        ));
    }
    let same_start = starts.iter().collect::<std::collections::BTreeSet<_>>()
        == holds.iter().collect::<std::collections::BTreeSet<_>>();
    Some(if same_start || !prefix.is_empty() {
        format!("{lead}{subject}may only contain {held}")
    } else {
        format!(
            "it must start with {} and may only contain {held}",
            starts.join(", ")
        )
    })
}

/// A pattern issue whose own schema has no note and a pattern too long to
/// read (`it must match ^(?:…`): the value and the enclosing field's `note`.
pub(super) fn describe_unreadable_pattern(issue: &mut ValidationIssue, note: &str) {
    let Some(schema) = issue.schema.as_ref() else {
        return;
    };
    let unreadable = issue.rule_id == "schema.pattern"
        && schema.get("description").is_none()
        && schema.get("format").is_none()
        && schema
            .get("pattern")
            .and_then(Value::as_str)
            .is_some_and(|pattern| {
                pattern.chars().count() > READABLE_PATTERN_CHARS
                    && stated_rule(pattern, schema).is_none()
            });
    if let (true, Some(text)) = (unreadable, issue.received.as_ref().and_then(Value::as_str))
        && !text.trim().is_empty()
    {
        let shown: String = text.chars().take(80).collect();
        issue.message = format!("\"{shown}\" does not match the expected form: {note}");
    }
}
