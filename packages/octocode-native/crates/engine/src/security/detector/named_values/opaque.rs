//! Which values are opaque: what a generator makes, not what a person
//! writes to be read.

use std::sync::LazyLock;

use super::names::letter_pieces;
use crate::security::detector::credential_values::{OPAQUE_LITERAL_CHARS, pronounceable_word};
use crate::security::patterns::OPAQUE_CHARS;

/// The bytes an opaque run is made of ([`OPAQUE_CHARS`], and base64's
/// padding).
static RUN_BYTES: LazyLock<[bool; 128]> = LazyLock::new(|| {
    let mut table = [false; 128];
    let class = OPAQUE_CHARS.as_bytes();
    let mut at = 0;
    while let Some(&byte) = class.get(at) {
        let range_end = class
            .get(at + 2)
            .filter(|_| class.get(at + 1) == Some(&b'-'));
        let end = range_end.copied().unwrap_or(byte);
        for member in byte..=end {
            if let Some(slot) = table.get_mut(usize::from(member)) {
                *slot = true;
            }
        }
        at += if range_end.is_some() { 3 } else { 1 };
    }
    table[usize::from(b'=')] = true;
    table
});

/// Whether every byte of `value` is an opaque run's.
pub(in crate::security::detector) fn is_run(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| RUN_BYTES.get(usize::from(byte)).copied().unwrap_or(false))
}

/// The evidence a value of [`ELEMENT_CHARS`] or more needs to be opaque.
const OPAQUE_EVIDENCE: usize = 7;

/// Pieces of letters that are no word which an opaque value holds: a name
/// holds an initial or an abbreviation or two (`getOAuthTokenV1`).
const OPAQUE_PIECES: usize = 3;

/// Letter-and-digit turns that say "opaque" whatever its letters spell: a
/// name numbers itself once or twice (`oauth2TokenV1`), a token turns
/// throughout.
const OPAQUE_TURNS: usize = 6;

/// Whether `value` is an opaque string of `min_chars` or more: what a
/// generator makes (a token, an encoded key, a generated password), not what
/// a person writes to be read. It mixes letters with digits or cases, repeats
/// and counts little (no `xxxx…`, no alphabet), and its shape is no word's:
/// evidence is each piece of letters that is no word (one or two letters, or
/// unpronounceable), each turn between a letter and a digit, and half of
/// each turn of case. Words joined by marks, a camelCase name, a path, a
/// version or a date gather little, and a name that only numbers its words
/// (`oauth2TokenV1ForUser`) has few pieces that are no word. A number
/// written as code writes one (`0x9E37_79B9_7F4A_7C15`, `0xff_u64`) is a
/// number.
pub(in crate::security::detector) fn is_opaque(value: &str, min_chars: usize) -> bool {
    let bytes = value.trim_end_matches('=').as_bytes();
    if bytes.len() < min_chars || !bytes.iter().all(u8::is_ascii_graphic) || is_number(bytes) {
        return false;
    }
    let digits = bytes.iter().any(u8::is_ascii_digit);
    let upper = bytes.iter().any(u8::is_ascii_uppercase);
    let lower = bytes.iter().any(u8::is_ascii_lowercase);
    if !((digits && (upper || lower)) || (upper && lower)) {
        return false;
    }
    let mut seen = [false; 128];
    for &byte in bytes {
        if let Some(slot) = seen.get_mut(usize::from(byte)) {
            *slot = true;
        }
    }
    let distinct = seen.iter().filter(|seen| **seen).count();
    let counted = bytes
        .windows(2)
        .filter(|pair| pair[1] == pair[0].wrapping_add(1))
        .count();
    if distinct < bytes.len().min(OPAQUE_LITERAL_CHARS) / 2 || counted * 2 > bytes.len() {
        return false;
    }
    let turns = |between: fn(&u8, &u8) -> bool| {
        bytes
            .windows(2)
            .filter(|pair| between(&pair[0], &pair[1]) || between(&pair[1], &pair[0]))
            .count()
    };
    let digit_turns = turns(|a, b| a.is_ascii_alphabetic() && b.is_ascii_digit());
    let case_turns = turns(|a, b| a.is_ascii_lowercase() && b.is_ascii_uppercase());
    let odd = letter_pieces(bytes)
        .into_iter()
        .filter(|piece| piece.len() <= 2 || !pronounceable_word(&piece.to_ascii_lowercase()))
        .count();
    odd + digit_turns + case_turns / 2 >= OPAQUE_EVIDENCE
        && (odd >= OPAQUE_PIECES || digit_turns >= OPAQUE_TURNS)
}

/// Whether `bytes` are a number as code writes one: a radix prefix, digits
/// of that radix with `_` between groups, and an integer type's suffix. A
/// key written in hex is longer than any integer ([`NUMBER_HEX_DIGITS`]).
fn is_number(bytes: &[u8]) -> bool {
    let Some(digits) = bytes
        .strip_prefix(b"0x")
        .or_else(|| bytes.strip_prefix(b"0X"))
    else {
        return false;
    };
    let suffix = digits
        .iter()
        .rposition(|byte| matches!(byte, b'u' | b'i'))
        .filter(|&at| {
            digits[at + 1..].iter().all(u8::is_ascii_digit) || &digits[at + 1..] == b"size"
        });
    let digits = suffix.map_or(digits, |at| &digits[..at]);
    let hex = digits
        .iter()
        .filter(|byte| byte.is_ascii_hexdigit())
        .count();
    (1..=NUMBER_HEX_DIGITS).contains(&hex)
        && digits
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() || *byte == b'_')
}

/// Hex digits of the widest integer code writes (128 bits).
const NUMBER_HEX_DIGITS: usize = 32;
