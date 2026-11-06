//! What ranking and the top-match lead need of a provider text match, and no
//! more of its text. A fragment's edges are the index's, not a line's or a
//! token's: a secret they cut can lose the prefix that names it, so no fragment
//! text leaves here but the match GitHub highlighted (an occurrence of the
//! caller's own keyword). Rows list sealed blob lines instead.
use super::ranking::Term;
use crate::security::scan::{ContentScan, sanitize_provider_text};
use octocode_github::{ProviderError, TextMatch};
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
        let needle = crate::content::utf16_slice(
            &fragment.fragment,
            slice_index(*start, length),
            slice_index(*end, length),
        )
        .filter(|needle| !needle.is_empty())?;
        sanitized.contains(&needle).then_some(needle)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::security::scan::Passthrough;

    #[test]
    fn a_projected_snippet_keeps_its_first_highlight_only() {
        let fragment = "# Title\n\n// keep  spacing\nuse   needle;\nlet needle = 1;";
        let at = |needle_line: &str| {
            let start = fragment.find(needle_line).unwrap();
            json!({"indices": [start, start + 6]})
        };
        let text_match: TextMatch = serde_json::from_value(json!({
            "fragment": fragment,
            "matches": [at("needle;"), at("needle =")]}))
        .unwrap();
        let (value, _) = project(&text_match, "README.md", &[], &Passthrough)
            .unwrap()
            .unwrap();
        assert_eq!(value, json!({"highlight": "needle"}));
    }

    /// No projected value holds fragment text beyond the match GitHub
    /// highlighted: a fake token the fragment's edge cuts (its prefix gone, so
    /// no scanner can name it), whole at either edge, or split from its name
    /// by a line break, never shows any 8-byte piece of it (B45).
    #[test]
    fn a_projected_fragment_keeps_only_its_highlight() {
        let security = crate::security::ContentSecurity::new();
        let token = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
        let pieces = |text: &str| {
            (4..token.len() - 8)
                .filter_map(|at| token.get(at..at + 8))
                .filter(|piece| text.contains(piece))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        for cut in [0, 4, 9, 20] {
            let (after, before) = (
                token.get(cut..).expect("ascii"),
                token.get(..token.len() - cut).expect("ascii"),
            );
            let head = format!("{after}\nlet needle = 1;\n");
            let tail = format!("let needle = 1;\nconst T = \"{before}");
            let split = format!("let needle = 1;\nconst TOKEN =\n  \"{after}\";");
            for fragment in [head, tail, split] {
                let start = fragment.find("needle").expect("needle");
                let text_match: TextMatch = serde_json::from_value(json!({
                    "fragment": fragment,
                    "matches": [{"indices": [start, start + 6]}]}))
                .expect("match");
                let (value, _) = project(&text_match, "src/main.rs", &[], &security)
                    .expect("projected")
                    .expect("shown");
                let shown = value.to_string();
                assert!(pieces(&shown).is_empty(), "cut {cut}: {shown}");
                assert_eq!(value["highlight"], "needle", "cut {cut}: {shown}");
            }
        }
    }
}
