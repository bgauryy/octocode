//! Value references counted from the name tokens the main walk recorded.

use super::{GraphAccumulator, NameToken};

/// Per-name counts of identifier-kind tokens that reference a declared name,
/// skipping declaration names and call-edge callee tokens. Comments and string
/// contents are never identifier tokens, so they cannot count; Rust inline
/// format captures (`"{name}"` inside a macro) are the one string form that
/// names a binding and are counted. No scope resolution: equal names share a
/// count. `tokens` are the ones the main walk recorded, so this reads no tree.
pub(super) fn count_name_references(
    content: &str,
    acc: &GraphAccumulator,
    tokens: &[NameToken],
) -> std::collections::HashMap<String, u32> {
    let mut counts: std::collections::HashMap<String, u32> = acc
        .declarations
        .iter()
        .map(|declaration| (declaration.name.clone(), 0))
        .collect();
    if counts.is_empty() {
        return counts;
    }
    for token in tokens {
        let text = content.get(token.start..token.end);
        if !token.format {
            if !acc.non_reference_tokens.contains(&token.start)
                && let Some(count) = text.and_then(|text| counts.get_mut(text))
            {
                *count += 1;
            }
            continue;
        }
        for name in text.map(inline_format_captures).unwrap_or_default() {
            if let Some(count) = counts.get_mut(name) {
                *count += 1;
            }
        }
    }
    counts
}

/// Identifiers captured by a Rust format string: `name` in `{name}` or
/// `{name:?}`; `{{` escapes and positional `{0}`/`{}` are skipped.
pub(super) fn inline_format_captures(literal: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = literal;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(escaped) = rest.strip_prefix('{') {
            rest = escaped;
            continue;
        }
        let Some(end) = rest.find(['}', ':']) else {
            break;
        };
        let name = &rest[..end];
        if name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            names.push(name);
        }
        rest = &rest[end..];
    }
    names
}
