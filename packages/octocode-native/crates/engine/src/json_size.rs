//! JSON wire sizes without rendering, and retained heap estimates.
use serde::Serialize;
use std::io::{self, Write};
use std::mem::size_of;

/// Approximate allocator rounding and bookkeeping for one heap block.
const BLOCK_BYTES: usize = 16;

/// An object's entry: its hash, key and value, index slot and key allocation.
pub const OBJECT_ENTRY_BYTES: usize =
    size_of::<(u64, String, serde_json::Value)>() + size_of::<usize>() + BLOCK_BYTES;

/// About the heap below a JSON value's slot, including structural overhead.
/// Lengths, rather than capacities, keep a scan's budget a fact of its input.
/// Serialized bytes alone undercount objects with many small fields.
pub fn heap_bytes(value: &serde_json::Value) -> usize {
    use serde_json::Value;
    match value {
        Value::String(text) => BLOCK_BYTES.saturating_add(text.len()),
        Value::Array(items) => items.iter().fold(
            BLOCK_BYTES.saturating_add(items.len().saturating_mul(size_of::<Value>())),
            |bytes, item| bytes.saturating_add(heap_bytes(item)),
        ),
        Value::Object(fields) => fields.iter().fold(BLOCK_BYTES, |bytes, (key, field)| {
            bytes
                .saturating_add(OBJECT_ENTRY_BYTES)
                .saturating_add(key.len())
                .saturating_add(heap_bytes(field))
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

struct Counter {
    size: usize,
    utf16: bool,
}

/// UTF-16 units of UTF-8 `bytes`: one per scalar start byte, one more for
/// each four-byte scalar (a surrogate pair). Exact for any chunk that splits
/// at scalar boundaries or inside one: continuation bytes count nothing.
pub fn utf16_units_of_utf8(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .map(|&byte| usize::from(byte & 0xc0 != 0x80) + usize::from(byte >= 0xf0))
        .sum()
}

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.size += if self.utf16 {
            utf16_units_of_utf8(bytes)
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
    fn retained_json_weight_counts_payload_and_structural_overhead() {
        let text = "λ東京😀".repeat(128);
        let string = serde_json::json!(text);
        assert!(heap_bytes(&string) >= text.len());
        let array = serde_json::json!([null, null, string]);
        assert!(heap_bytes(&array) >= 3 * size_of::<serde_json::Value>() + text.len());
        let object = serde_json::json!({"small": 1, "nested": array});
        assert!(heap_bytes(&object) >= 2 * OBJECT_ENTRY_BYTES + heap_bytes(&array));
        let many_fields: serde_json::Value = serde_json::Value::Object(
            (0..128)
                .map(|index| (format!("k{index}"), serde_json::json!(0)))
                .collect(),
        );
        assert!(heap_bytes(&many_fields) > bytes(&many_fields).expect("wire bytes"));
    }

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
