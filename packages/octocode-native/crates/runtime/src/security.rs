mod content;
pub(crate) mod cut;
mod input;
mod output;
pub(crate) mod scan;
mod walk;

pub use content::ContentSecurity;
pub(crate) use content::{
    BlockLine, KeyBlockState, KeyBlockTracker, LineShape, LineShapeScan, is_key_header_prefix,
    key_fragment_placeholder, match_window_intersects_key_block, private_key_block_line_ranges,
    redact_private_key_blocks, snippet_may_hold_key_material,
};
pub(crate) use input::{CheckedQuery, InputPolicy};
pub use output::Sanitized;
pub(crate) use walk::sanitize_json;

/// Secret-scrubbed host-boundary error text. A secret echoed by a remote
/// server or embedded in a provider payload is redacted; a sanitizer failure
/// fails closed to a redaction placeholder.
pub fn scrub_error_text(text: &str) -> String {
    octocode_engine::security::sanitizer::sanitize_content(text, None)
        .map(|result| result.content)
        .unwrap_or_else(|_| octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned())
}
