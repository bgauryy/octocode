mod content;
pub(crate) mod cut;
mod input;
mod limits;
mod output;
pub(crate) mod scan;
mod walk;

pub use content::ContentSecurity;
#[cfg(test)]
pub(crate) use content::tests::{key_formats, marker_lines};
pub(crate) use content::{
    KeyBlockState, KeyBlockTracker, LineShapeScan, MarkerScan, hide_key_line,
    key_fragment_placeholder, key_line_lead, lines_a_row_cannot_judge,
    match_window_intersects_key_block, private_key_block_line_ranges, redact_private_key_blocks,
    snippet_may_hold_key_material,
};
pub(crate) use input::{CheckedQuery, InputPolicy};
pub use limits::SourceLimits;
pub use output::Sanitized;
pub(crate) use walk::sanitize_json;

/// The one warning that returned text is not verbatim source: `values`
/// secret-shaped values were replaced by placeholders (`0`: an unknown number,
/// as when a page shows only a blanked line of one redaction). Every tool
/// states a redaction with it, whichever stage redacted.
pub(crate) fn redacted_content_warning(values: usize) -> String {
    if values > 0 {
        format!(
            "redactedContent: {values} secret-shaped value(s) in the returned text were replaced by [REDACTED…] placeholders; this content is not verbatim source."
        )
    } else {
        "redactedContent: secret-shaped text in the requested view was redacted; this content is not verbatim source.".into()
    }
}

/// Redaction placeholders in `text` (literal ones in a source count too).
pub(crate) fn placeholders(text: &str) -> usize {
    text.matches(octocode_engine::security::REDACTION_PREFIX)
        .count()
}

/// Redaction placeholders in every string leaf and key of `value`.
pub(crate) fn placeholders_in(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(text) => placeholders(text),
        serde_json::Value::Array(items) => items.iter().map(placeholders_in).sum(),
        serde_json::Value::Object(fields) => fields
            .iter()
            .map(|(key, child)| placeholders(key) + placeholders_in(child))
            .sum(),
        _ => 0,
    }
}

/// Secret-scrubbed host-boundary error text. A secret echoed by a remote
/// server or embedded in a provider payload is redacted; a sanitizer failure
/// fails closed to a redaction placeholder.
pub fn scrub_error_text(text: &str) -> String {
    octocode_engine::security::sanitizer::sanitize_content(text, None)
        .map(|result| result.content.into_owned())
        .unwrap_or_else(|_| octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
}
