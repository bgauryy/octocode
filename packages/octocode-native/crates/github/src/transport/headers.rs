//! Delimited HTTP field values, retaining separators inside quoted strings and URIs.

pub(super) fn list(value: &str, separator: char) -> impl Iterator<Item = &str> {
    let mut quoted = false;
    let mut escaped = false;
    let mut uri = false;
    value
        .split(move |character| {
            if escaped {
                escaped = false;
                return false;
            }
            match character {
                '\\' if quoted => escaped = true,
                '"' if !uri => quoted = !quoted,
                '<' if !quoted => uri = true,
                '>' if !quoted => uri = false,
                _ => {}
            }
            character == separator && !quoted && !uri
        })
        .map(str::trim)
        .filter(|part| !part.is_empty())
}
