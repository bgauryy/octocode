//! What a walk reads of the text around its values: where a line, a field
//! and a string end, and an armor that names its body as public.

use super::opaque::{is_opaque, is_run};
use crate::security::detector::credential_values::{OPAQUE_LITERAL_CHARS, escaped_control};

/// Whether `body` holds an armor that names what it encloses as public: a
/// public key or a certificate under a key's name is published, and a
/// private key beside it is the key rules' own.
pub(super) fn is_public_armor(body: &str) -> bool {
    body.contains("-----BEGIN ")
        && ["PUBLIC KEY", "CERTIFICATE"]
            .iter()
            .any(|kind| body.contains(kind))
}

/// Where the line of `at` ends in `bytes`: its line break, or the end.
pub(super) fn line_end(bytes: &[u8], at: usize) -> usize {
    bytes
        .get(at..)
        .and_then(|rest| rest.iter().position(|byte| matches!(byte, b'\n' | b'\r')))
        .map_or(bytes.len(), |end| at + end)
}

/// Where `value`, read up to a quote or a blank, ends at a control-character
/// escape inside it ([`escaped_control`]: `\r\n`, `\t`, `\u000a`): what
/// precedes the escape is one opaque run ([`is_run`], [`is_opaque`]), and no
/// word after it is opaque, so the text there is the next field's (`<token>\r\nHost:
/// example.com`), not more of one value written over lines (a key's base64
/// body, `<line>\n<line>`). `None` when the value runs whole: the owning rule
/// then hides all of it. The caller still judges the part before as its rule
/// judges a value.
pub(in crate::security::detector) fn field_end(value: &str) -> Option<usize> {
    field_end_after(value, |before| {
        is_run(before) && is_opaque(before, OPAQUE_LITERAL_CHARS)
    })
}

/// [`field_end`] for a value whose part before the escape `before` accepts.
pub(in crate::security::detector) fn field_end_after(
    value: &str,
    before: impl Fn(&str) -> bool,
) -> Option<usize> {
    let at = escaped_control(value)?;
    let (head, after) = (value.get(..at)?, value.get(at..)?);
    let opaque_after = after
        .split(|c: char| c == '\\' || c.is_whitespace())
        .map(|piece| piece.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .any(|piece| is_opaque(piece, 0));
    (before(head) && !opaque_after).then_some(at)
}

/// Whether the byte at `at` in `bytes` is the letter of a line-ending escape
/// (`\n`, `\r`): an `n` or `r` after an odd run of backslashes.
pub(super) fn ends_a_line(bytes: &[u8], at: usize) -> bool {
    matches!(bytes.get(at), Some(b'n' | b'r'))
        && bytes
            .get(..at)
            .unwrap_or_default()
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count()
            % 2
            == 1
}

/// Where the body of the string opened by the quote at `open` ends: at its
/// closing quote, an escape stepped over, or for a `"` or `'` string at its
/// line's end.
pub(super) fn string_end(bytes: &[u8], open: usize) -> usize {
    let quote = bytes[open];
    let mut at = open + 1;
    while let Some(&byte) = bytes.get(at) {
        if byte == quote || (quote != b'`' && matches!(byte, b'\n' | b'\r')) {
            return at;
        }
        at += if byte == b'\\' { 2 } else { 1 };
    }
    bytes.len()
}
