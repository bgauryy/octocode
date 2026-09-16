//! Preserve text-match anchors through the shared native sanitizer/minifier.
use crate::{
    providers::github::{ProviderError, ProviderErrorKind, TextMatch},
    tools::local_fetch::ContentScan,
};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn project(
    fragment: &TextMatch,
    path: &str,
    security: &impl ContentScan,
) -> Result<Option<Value>, ProviderError> {
    let sanitized = security
        .sanitize(&fragment.fragment, Path::new(path))
        .map_err(|(message, _)| ProviderError::new(ProviderErrorKind::Validation, message))?
        .0;
    let original_positions = positions(fragment, &sanitized);
    let compact = octocode_engine_core::portable::minify_content(&sanitized, path);
    let compact_positions = positions(fragment, &compact.content);
    let (text, anchors) = if !compact.failed && compact_positions.len() == original_positions.len()
    {
        (compact.content, compact_positions)
    } else {
        (sanitized, original_positions)
    };
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
        length.saturating_sub(index.unsigned_abs() as usize)
    } else {
        (index as usize).min(length)
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
