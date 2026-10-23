//! Serialized JSON sizes without allocating the rendered document.
use serde::Serialize;
use std::io::{self, Write};

struct Counter {
    size: usize,
    utf16: bool,
}

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.size += if self.utf16 {
            bytes
                .iter()
                .map(|&byte| usize::from(byte & 0xc0 != 0x80) + usize::from(byte >= 0xf0))
                .sum::<usize>()
        } else {
            bytes.len()
        };
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn count(value: &(impl Serialize + ?Sized), utf16: bool) -> Option<usize> {
    let mut counter = Counter { size: 0, utf16 };
    serde_json::to_writer(&mut counter, value).ok()?;
    Some(counter.size)
}

/// JSON bytes, or `None` if serialization failed; a partial document has no size.
pub fn bytes(value: &(impl Serialize + ?Sized)) -> Option<usize> {
    count(value, false)
}

/// JSON UTF-16 units, including escaping, or `None` if serialization failed.
pub fn utf16_units(value: &(impl Serialize + ?Sized)) -> Option<usize> {
    count(value, true)
}

/// [`bytes`], or `fallback` when serialization failed: the one failure
/// policy. Pick `usize::MAX` where an unmeasured value must not count as
/// small (a size bound, a cache weight); sums over it saturate.
pub fn bytes_or(value: &(impl Serialize + ?Sized), fallback: usize) -> usize {
    bytes(value).unwrap_or(fallback)
}

/// [`utf16_units`], or `fallback` when serialization failed; see [`bytes_or`].
pub fn utf16_units_or(value: &(impl Serialize + ?Sized), fallback: usize) -> usize {
    utf16_units(value).unwrap_or(fallback)
}

/// UTF-16 units of `text` as a JSON string body: escaped, unquoted. A string
/// always serializes, so there is no failure to price.
pub fn string_body_units(text: &str) -> usize {
    utf16_units_or(text, 2).saturating_sub(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_size_matches_rendered_bytes_and_utf16() {
        for value in [
            serde_json::json!(null),
            serde_json::json!("quote \" tab \t é 中文 😀"),
            serde_json::json!({"nested": [1, true, "\n"]}),
            serde_json::json!({"message": "héllo \"quoted\"", "items": [1, 2.5, null, {"k": "v"}]}),
        ] {
            let rendered = serde_json::to_string(&value).unwrap();
            assert_eq!(bytes(&value), Some(rendered.len()));
            assert_eq!(utf16_units(&value), Some(rendered.encode_utf16().count()));
        }
        let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
        assert_eq!(bytes(&invalid), None);
        assert_eq!(utf16_units(&invalid), None);
        assert_eq!(bytes_or(&invalid, usize::MAX), usize::MAX);
        assert_eq!(utf16_units_or(&invalid, 7), 7);
        assert_eq!(bytes_or(&value_of("a"), usize::MAX), 3);
    }

    fn value_of(text: &str) -> serde_json::Value {
        serde_json::json!(text)
    }

    #[test]
    fn a_string_body_counts_escapes_and_utf16_units_as_rendered() {
        assert_eq!(string_body_units("a\"b\u{1F600}"), 6);
        assert_eq!(string_body_units(""), 0);
        assert_eq!(string_body_units("tab\t"), 5);
    }
}
