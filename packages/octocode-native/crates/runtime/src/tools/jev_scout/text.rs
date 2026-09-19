//! Pure formatting/text primitives for jevScout.
//!
//! The relationship taxonomy, secrets redaction, JS-compatible UTF-16 width,
//! boundary-safe truncation, and small numeric/JSON helpers. No request state,
//! sandbox logic, or provider coupling lives here — these are copied verbatim
//! from the frozen `scout.mjs` reference.

use serde_json::{Value, json};

/// Default relationship taxonomy — byte-identical to scout.mjs `DEFAULT_LEVELS`.
pub(super) fn default_levels() -> Vec<Value> {
    vec![
        json!({ "level": "none", "meaning": "no relation to the capability" }),
        json!({ "level": "mentions", "meaning": "keywords appear but nothing is used" }),
        json!({ "level": "imports", "meaning": "imports or calls the capability defined elsewhere" }),
        json!({ "level": "implements", "meaning": "defines the capability itself in this file" }),
    ]
}

/// The same secrets redaction the localFetch read path applies. Fails closed on a
/// pathological-input sanitizer panic, mirroring `ContentSecurity::sanitize_text`.
pub(super) fn redact(text: &str) -> String {
    match octocode_engine::portable::sanitize_content(text, None) {
        Ok(result) => result.content,
        Err(_) => "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned(),
    }
}

/// JS string length semantics: UTF-16 code units (matches `String.prototype.length`).
pub(super) fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Prefix of `text` up to `max_units` UTF-16 code units, never splitting a UTF-8
/// code point. For BMP text this equals JS `slice(0, max_units)`; see module docs.
pub(super) fn truncate_utf16(text: &str, max_units: usize) -> String {
    if max_units == 0 {
        return String::new();
    }
    let mut units = 0usize;
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let width = character.len_utf16();
        if units + width > max_units {
            break;
        }
        units += width;
        end = index + character.len_utf8();
    }
    text[..end].to_owned()
}

pub(super) fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

pub(super) fn is_object(value: &Value) -> bool {
    value.is_object()
}
