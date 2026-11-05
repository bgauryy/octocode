//! Preserve the context of actual JSON members without treating serialized
//! source-code strings as configuration. The canonical patterns identify a
//! member's value; the existing object walker identifies Secret/JWK members.
//! Only those data values change. Keys and text inside values retain the usual
//! leaf scanner, with its own source-file context.
use super::{Scope, applicable_patterns, document::Form, long_text, objects, replacement_for};
use serde_json::Value;
use std::cell::LazyCell;
use std::ops::Range;

/// One slot per serialized JSON string, keys included. A slot names the
/// canonical pattern that made this whole member value secret.
pub(in crate::security) fn matches(value: &Value) -> crate::error::Result<Vec<Option<usize>>> {
    if !matches!(value, Value::Object(_) | Value::Array(_)) {
        return Ok(Vec::new());
    }
    let text = serde_json::to_string(value)
        .map_err(|_| crate::error::Error::new("structured content could not be scanned"))?;
    let candidates = applicable_patterns(&text, Scope::Stored(None));
    // Clean payloads need neither a string-position index nor a rewrite map.
    let tokens = LazyCell::new(|| strings(&text));
    let mut hidden = Vec::new();
    let mut objects = None;
    for idx in candidates.iter() {
        match Form::of(idx) {
            Some(Form::SecretInYaml) => {}
            Some(form) => {
                let found = objects.get_or_insert_with(|| objects::walk(&text));
                let ranges = if form == Form::SecretInJson {
                    &found.secret_values
                } else {
                    &found.private_members
                };
                for range in ranges {
                    let first = tokens.partition_point(|token| token.end <= range.start);
                    for (at, token) in tokens.iter().enumerate().skip(first) {
                        if token.start >= range.end {
                            break;
                        }
                        if range.start <= token.start && range.end >= token.end {
                            hidden.resize(tokens.len(), None);
                            hidden[at].get_or_insert(idx);
                        }
                    }
                }
            }
            None => long_text::pattern_matches(idx, &text, true, |whole, secret| {
                let at = tokens.partition_point(|token| token.end <= secret.start);
                if let Some(token) = tokens.get(at)
                    && secret.start == token.start
                    && whole.start < token.start.saturating_sub(1)
                {
                    // The assignment starts outside the string: its JSON key
                    // supplies the context. A match within a source-code string
                    // must instead be judged by that leaf's source-file rules.
                    hidden.resize(tokens.len(), None);
                    hidden[at].get_or_insert(idx);
                }
            }),
        }
    }
    Ok(hidden)
}

/// String interiors in serde's own valid JSON, in serialization order. This is
/// a lexical index, not a JSON parser: serde owns escaping and structure.
fn strings(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut strings = Vec::new();
    while at < bytes.len() {
        if bytes[at] != b'"' {
            at += 1;
            continue;
        }
        at += 1;
        let start = at;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 2,
                b'"' => break,
                _ => at += 1,
            }
        }
        strings.push(start..at);
        at += 1;
    }
    strings
}

pub(in crate::security) fn apply(
    value: &mut Value,
    hidden: Vec<Option<usize>>,
) -> Vec<(String, String)> {
    let mut removed = Vec::new();
    if hidden.iter().any(Option::is_some) {
        walk(value, &mut hidden.into_iter(), &mut removed);
    }
    removed
}

fn walk(
    value: &mut Value,
    hidden: &mut impl Iterator<Item = Option<usize>>,
    removed: &mut Vec<(String, String)>,
) {
    match value {
        Value::String(text) => {
            if let Some(Some(idx)) = hidden.next() {
                let placeholder = replacement_for(idx);
                if text != placeholder {
                    let original = std::mem::replace(text, placeholder.to_owned());
                    removed.push((original, placeholder.to_owned()));
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                walk(child, hidden, removed);
            }
        }
        Value::Object(values) => {
            for child in values.values_mut() {
                // The key stays a key; the normal walker scans key text and
                // resolves collisions without losing entries.
                hidden.next();
                walk(child, hidden, removed);
            }
        }
        _ => {}
    }
}
