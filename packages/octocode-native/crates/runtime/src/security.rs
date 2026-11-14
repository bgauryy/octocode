mod content;
pub(crate) mod cut;
mod input;
mod judged;
mod limits;
mod output;
pub(crate) mod scan;
mod walk;

pub use content::ContentSecurity;
pub(crate) use content::Pii;
#[cfg(test)]
pub(crate) use content::tests::{key_formats, marker_lines};
pub(crate) use content::{
    KEY_LINE_HEAD_BYTES, KeyBlockState, KeyBlockTracker, LineShapeScan, MarkerScan, hide_key_line,
    key_fragment_placeholder, key_line_bytes, key_line_lead, match_window_intersects_key_block,
    private_key_block_line_ranges, private_key_blocks_hidden, redact_private_key_blocks,
    snippet_may_hold_key_material,
};
pub(crate) use input::{Admitted, CheckedQuery, InputPolicy};
#[cfg(test)]
pub(crate) use judged::scanned_on_this_thread;
pub(crate) use judged::{
    JudgedFiles, JudgedLeaf, JudgedLeaves, JudgedSource, JudgedText, WholePass, meets_written,
    searched_text,
};
pub use limits::SourceLimits;
pub use output::Sanitized;
pub(crate) use walk::sanitize_json;

/// Outside text a tool message quotes (a provider's error, a language
/// server's stderr or diagnostic, a registry's or a module's reason, a
/// file's rule or path, a parser's message), judged where the message is
/// built, by the net every text of no known file gets: a credential in it
/// leaves as the placeholder a read shows, the rest as written. Every
/// message that quotes outside text quotes it through this; the output
/// gate's pathless pass over messages ([`walk::Messages::Authored`]) is the
/// backstop until every site does.
pub(crate) fn quoted(text: &str) -> std::borrow::Cow<'_, str> {
    match ContentSecurity::new().sanitize_leaf(text) {
        Some(clean) => std::borrow::Cow::Owned(clean),
        None => std::borrow::Cow::Borrowed(text),
    }
}

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

/// The values a [`redacted_content_warning`] counts; `None` for any other
/// warning, or one that counts none: parts of one row joined state their sum.
pub(crate) fn redacted_values(warning: &str) -> Option<usize> {
    let values = warning
        .strip_prefix("redactedContent: ")?
        .split_once(' ')?
        .0
        .parse()
        .ok()?;
    (values > 0 && redacted_content_warning(values) == warning).then_some(values)
}

/// Whether `warning` states that a row's own text was redacted
/// ([`redacted_content_warning`], localSearch's `redactedMatches`): a fact of
/// that row, with that row's count, never of the call.
pub(crate) fn states_a_redaction(warning: &str) -> bool {
    warning.starts_with("redacted")
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
    octocode_engine::security::sanitizer::sanitize_content(text)
        .map(|result| result.content.into_owned())
        .unwrap_or_else(|_| octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
}
