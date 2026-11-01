//! Preserve text-match anchors through the shared native sanitizer. Snippets
//! are already small, so they retain their original lines, spacing, and
//! comments are the evidence, and each match keeps its own line offset.
use crate::security::scan::{ContentScan, sanitize_provider_text};
use octocode_github::{ProviderError, TextMatch};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn project(
    fragment: &TextMatch,
    path: &str,
    security: &impl ContentScan,
) -> Result<Option<Value>, ProviderError> {
    let sanitized = sanitize_provider_text(security, &fragment.fragment, Path::new(path))?;
    let anchors = positions(fragment, &sanitized);
    let text = sanitized;
    if text.is_empty() {
        return Ok(None);
    }
    let mut value = json!({"value":text});
    if !anchors.is_empty() {
        value["matchIndices"] = json!(anchors);
    }
    Ok(Some(value))
}

fn slice_index(index: i64, length: usize) -> usize {
    if index < 0 {
        length.saturating_sub(crate::num::usize_of(index.unsigned_abs()))
    } else {
        usize::try_from(index).map_or(length, |index| index.min(length))
    }
}

fn positions(fragment: &TextMatch, transformed: &str) -> Vec<Value> {
    let raw: Vec<u16> = fragment.fragment.encode_utf16().collect();
    let output: Vec<u16> = transformed.encode_utf16().collect();
    let mut from = 0;
    let mut result = Vec::new();
    for position in &fragment.matches {
        let [start, end, ..] = position.indices.as_slice() else {
            continue;
        };
        let start = slice_index(*start, raw.len());
        let end = slice_index(*end, raw.len());
        if end <= start {
            continue;
        }
        let needle = &raw[start..end];
        // Unchanged text keeps GitHub's exact indices (repeated needles stay
        // on the right occurrence); redacted text is re-anchored by search.
        if output.get(start..end) == Some(needle) && output.len() == raw.len() {
            result.push(json!({"start":start,"end":end,"lineOffset":output[..start].iter().filter(|&&c| c == 10).count()}));
            from = end;
            continue;
        }
        let find = |offset: usize| {
            output[offset..]
                .windows(needle.len())
                .position(|v| v == needle)
                .map(|i| i + offset)
        };
        let Some(index) = find(from).or_else(|| find(0)) else {
            continue;
        };
        result.push(json!({"start":index,"end":index+needle.len(),"lineOffset":output[..index].iter().filter(|&&c| c == 10).count()}));
        from = index + needle.len();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::security::scan::Passthrough;

    #[test]
    fn snippets_stay_raw_and_each_match_keeps_its_line() {
        let fragment = "# Title\n\n// keep  spacing\nuse   needle;\nlet needle = 1;";
        let at = |needle_line: &str| {
            let start = fragment.find(needle_line).unwrap();
            json!({"indices": [start, start + 6]})
        };
        let text_match: TextMatch = serde_json::from_value(json!({
            "fragment": fragment,
            "matches": [at("needle;"), at("needle =")]}))
        .unwrap();
        let value = project(&text_match, "README.md", &Passthrough)
            .unwrap()
            .unwrap();
        assert_eq!(value["value"], fragment, "snippet must not be minified");
        assert_eq!(value["matchIndices"][0]["lineOffset"], 3);
        assert_eq!(value["matchIndices"][1]["lineOffset"], 4);
    }
}
