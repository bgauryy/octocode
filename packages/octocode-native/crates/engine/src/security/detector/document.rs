//! The patterns a document's shape decides. A value's secrecy lies in another member
//! of its object, at any distance, so no pattern over the text can pair them without
//! a limit that a longer document walks past. Their regex says only that a text names
//! what makes them apply; a walk of its structure then finds what they hide.
use super::{jwk, objects, yaml_secret};
use crate::security::patterns::PATTERNS;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    SecretInYaml,
    SecretInJson,
    JsonWebKey,
}

impl Form {
    /// The form of pattern `idx`, when its shape is what decides it.
    pub(super) fn of(idx: usize) -> Option<Self> {
        match PATTERNS[idx].name {
            "kubernetesSecrets" => Some(Self::SecretInYaml),
            "kubernetesSecretJson" => Some(Self::SecretInJson),
            jwk::PATTERN => Some(Self::JsonWebKey),
            _ => None,
        }
    }

    /// `text` with what this form names replaced, line for line.
    pub(super) fn redact(self, text: &str, placeholder: &str) -> String {
        let ranges = match self {
            Self::SecretInYaml => yaml_secret::secret_blocks(text),
            Self::SecretInJson => objects::walk(text).secret_values,
            Self::JsonWebKey => objects::walk(text).private_members,
        };
        replace_ranges(text, ranges, placeholder)
    }
}

/// `text` with each of `ranges` replaced by `placeholder`, keeping its line count.
/// A range inside another goes with it; one holding only blanks hides nothing.
fn replace_ranges(text: &str, mut ranges: Vec<Range<usize>>, placeholder: &str) -> String {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for range in ranges {
        if range.end <= from || text[range.clone()].trim().is_empty() {
            continue;
        }
        let start = range.start.max(from);
        out.push_str(&text[from..start]);
        if range.start >= from {
            out.push_str(placeholder);
        }
        let hidden = &text[start..range.end];
        let line_breaks = hidden.bytes().filter(|byte| *byte == b'\n').count();
        out.extend(std::iter::repeat_n('\n', line_breaks));
        from = range.end;
    }
    out.push_str(&text[from..]);
    out
}
