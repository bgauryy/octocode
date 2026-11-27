mod content;
pub mod cut;
mod input;
mod judged;
#[cfg(any(test, feature = "test-support"))]
pub mod key_formats;
mod limits;
#[cfg(any(test, feature = "test-support"))]
pub mod marker_lines;
mod output;
pub mod scan;
mod walk;

pub use content::{
    ContentSecurity, KEY_LINE_HEAD_BYTES, KeyBlockState, KeyBlockTracker, LineShapeScan,
    MarkerScan, Pii, hide_key_line, key_fragment_placeholder, key_line_bytes, key_line_lead,
    match_window_intersects_key_block, private_key_block_line_ranges, private_key_blocks_hidden,
    redact_private_key_blocks, row_shown_unchecked, snippet_may_hold_key_material,
};
pub use input::{Admitted, CheckedQuery, InputPolicy};
#[cfg(any(test, feature = "test-support"))]
pub use judged::scanned_on_this_thread;
pub use judged::{
    JudgedFiles, JudgedLeaf, JudgedLeaves, JudgedSource, JudgedText, WholePass, meets_written,
    searched_text,
};
pub use limits::SourceLimits;
pub use output::Sanitized;
pub use walk::sanitize_json;

/// Outside text a tool message quotes (a provider's error, a language
/// server's stderr or diagnostic, a registry's or a module's reason, a
/// file's rule or path, a parser's message), judged where the message is
/// built, by the net every text of no known file gets: a credential in it
/// leaves as the placeholder a read shows, the rest as written. Every
/// message that quotes outside text quotes it through this; the output
/// gate's pathless pass over messages ([`walk::Messages::Authored`]) is the
/// backstop until every site does.
pub fn quoted(text: &str) -> std::borrow::Cow<'_, str> {
    match ContentSecurity::new().sanitize_leaf(text) {
        Some(clean) => std::borrow::Cow::Owned(clean),
        None => std::borrow::Cow::Borrowed(text),
    }
}

/// What a tool puts in its warnings plumbing when text it returns was
/// replaced by placeholders: never shown. The output gate turns it into the
/// row's `redacted` flag ([`crate::response::rows::warn_redacted_rows`]), the
/// one statement of a redaction.
pub const REDACTED_MARK: &str = "\u{1}redacted";

/// [`REDACTED_MARK`] as a warning.
pub fn redacted_mark() -> String {
    REDACTED_MARK.to_owned()
}

/// Sets the row's `redacted` flag: secret-shaped text in `data` was replaced
/// by placeholders and the rest is verbatim.
pub fn mark_redacted(data: &mut serde_json::Value) {
    if let Some(fields) = data.as_object_mut() {
        fields.insert("redacted".into(), serde_json::Value::Bool(true));
    }
}

/// Redaction placeholders in `text` (literal ones in a source count too).
pub fn placeholders(text: &str) -> usize {
    text.matches(octocode_engine::security::REDACTION_PREFIX)
        .count()
}

/// Redaction placeholders in every string leaf and key of `value`.
pub fn placeholders_in(value: &serde_json::Value) -> usize {
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
    octocode_engine::security::sanitizer::sanitize_content(text)
        .map(|result| result.content.into_owned())
        .unwrap_or_else(|_| octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
}
