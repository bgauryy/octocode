//! What ranking and the top-match lead need of a provider text match, and no
//! more of its text. A fragment's edges are the index's, not a line's or a
//! token's: a secret they cut can lose the prefix that names it, so no fragment
//! text leaves here but the match GitHub highlighted (an occurrence of the
//! caller's own keyword). Rows list sealed blob lines instead.
use super::ranking::Term;
use octocode_github::{ProviderError, TextMatch};
use octocode_runtime_base::security::scan::{ContentScan, sanitize_provider_text};
use serde_json::{Value, json};
use std::path::Path;

/// One fragment of `path`: `{"highlight": <first highlighted match>}` (or
/// `{}`), with the score of its sanitized text against `terms`; `None` when
/// sanitizing leaves no text. The scanner's refusal is the error.
pub(super) fn project(
    fragment: &TextMatch,
    path: &str,
    terms: &[Term],
    security: &impl ContentScan,
) -> Result<Option<(Value, u8)>, ProviderError> {
    let sanitized = sanitize_provider_text(security, &fragment.fragment, Path::new(path))?;
    if sanitized.is_empty() {
        return Ok(None);
    }
    let score = super::ranking::score(path, &sanitized, terms);
    let mut value = json!({});
    if let Some(highlight) = highlight(fragment, &sanitized) {
        value["highlight"] = json!(highlight);
    }
    Ok(Some((value, score)))
}

fn slice_index(index: i64, length: usize) -> usize {
    if index < 0 {
        length.saturating_sub(octocode_engine::num::saturating_usize(index.unsigned_abs()))
    } else {
        usize::try_from(index).map_or(length, |index| index.min(length))
    }
}

/// The first match GitHub highlighted (UTF-16 indices into the raw fragment)
/// that the sanitized text still shows whole: a match a redaction covered is
/// not shown.
fn highlight(fragment: &TextMatch, sanitized: &str) -> Option<String> {
    let length = fragment.fragment.encode_utf16().count();
    fragment.matches.iter().find_map(|position| {
        let [start, end, ..] = position.indices.as_slice() else {
            return None;
        };
        let needle = octocode_runtime_base::content::utf16_slice(
            &fragment.fragment,
            slice_index(*start, length),
            slice_index(*end, length),
        )
        .filter(|needle| !needle.is_empty())?;
        sanitized.contains(&needle).then_some(needle)
    })
}

#[cfg(test)]
mod tests;
