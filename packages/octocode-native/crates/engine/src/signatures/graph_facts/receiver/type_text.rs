//! Type text reduced to a type name: annotations, references, pointers and
//! generic arguments are stripped.

use super::*;

/// A Python annotation as a type name: string forward references are read,
/// `Optional[T]` and `T | None` unwrap to `T`, subscripts are dropped.
pub(in crate::signatures::graph_facts) fn python_type_text(text: &str) -> Option<String> {
    let mut text = text.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|t| t.strip_suffix(quote)) {
            text = inner.trim();
        }
    }
    for prefix in ["Optional[", "typing.Optional["] {
        if let Some(inner) = text.strip_prefix(prefix).and_then(|t| t.strip_suffix(']')) {
            return python_type_text(inner);
        }
    }
    let parts: Vec<&str> = split_top_level(text, '|')
        .into_iter()
        .map(str::trim)
        .filter(|part| *part != "None")
        .collect();
    match parts.as_slice() {
        [only] if *only != text => python_type_text(only),
        [only] => clean_generic(only, '[', ']').filter(|ty| ty.split('.').all(is_identifier)),
        _ => None,
    }
}

/// A type as written, reduced to its (possibly qualified) name: references,
/// pointers, `mut`/`const`/`dyn`/`impl`, lifetimes, trailing `?`, auto-trait
/// bounds and generic arguments are stripped; Rust `Box`/`Rc`/`Arc` unwrap to
/// their content. Arrays, slices, tuples, maps and function types are `None`.
pub(in crate::signatures::graph_facts) fn clean_type_text(
    text: &str,
    lang: Lang,
) -> Option<String> {
    let mut text = text.trim();
    loop {
        let before = text;
        for prefix in [
            "&",
            "*",
            "mut ",
            "const ",
            "dyn ",
            "impl ",
            "struct ",
            "class ",
            "enum ",
            "typename ",
        ] {
            if let Some(rest) = text.strip_prefix(prefix) {
                text = rest.trim_start();
            }
        }
        if text.starts_with('\'') {
            // A Rust lifetime: `&'a T`.
            text = text
                .split_once(' ')
                .map_or("", |(_, rest)| rest)
                .trim_start();
        }
        text = text.trim_end_matches(['?', '*', '&']).trim_end();
        if text == before {
            break;
        }
    }
    let text = split_top_level(text, '+').first()?.trim();
    let (open, close) = match lang {
        Lang::Go | Lang::Python => ('[', ']'),
        _ => ('<', '>'),
    };
    if lang == Lang::Rust
        && let Some((base, arguments)) = generic_parts(text, open, close)
        && matches!(last_segment(base), "Box" | "Rc" | "Arc")
        && let [inner] = split_top_level(arguments, ',').as_slice()
    {
        return clean_type_text(inner, lang);
    }
    let name = clean_generic(text, open, close)?;
    let separator = if matches!(lang, Lang::Rust | Lang::Cpp) {
        "::"
    } else {
        "."
    };
    name.split(separator).all(is_identifier).then_some(name)
}

/// `base` and the argument text of `base<args>`.
fn generic_parts(text: &str, open: char, close: char) -> Option<(&str, &str)> {
    let start = text.find(open)?;
    let inner = text[start + 1..].strip_suffix(close)?;
    Some((text[..start].trim_end_matches("::"), inner))
}

/// `text` without a trailing generic argument list.
fn clean_generic(text: &str, open: char, close: char) -> Option<String> {
    if text.starts_with(open) {
        return None;
    }
    let base = match text.find(open) {
        Some(start) => {
            text.ends_with(close).then_some(())?;
            text[..start].trim_end_matches("::")
        }
        None => text,
    };
    (!base.is_empty()).then(|| base.to_owned())
}

/// Split at `separator` outside `<>`, `[]` and `()`.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut depth = 0i32;
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        match ch {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            _ if ch == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

pub(super) fn is_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
}
