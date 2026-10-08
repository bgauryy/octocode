mod content;
pub(crate) mod scan;
mod walk;

pub use content::{ContentSecurity, ValidationResult};
pub(crate) use content::{
    KeyBlockTracker, key_fragment_placeholder, match_window_intersects_key_block,
    private_key_block_line_ranges, redact_private_key_blocks, snippet_may_hold_key_material,
};
pub use walk::sanitize_json;

/// Secret-scrubbed host-boundary error text. A secret echoed by a remote
/// server or embedded in a provider payload is redacted; a sanitizer failure
/// fails closed to a redaction placeholder.
pub fn scrub_error_text(text: &str) -> String {
    octocode_engine::portable::sanitize_content(text, None)
        .map(|result| result.content)
        .unwrap_or_else(|_| "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned())
}

/// [`scrub_error_text`] over every string leaf of an error payload.
pub fn scrub_error_payload(payload: &mut serde_json::Value) {
    let _ = sanitize_json(payload, &mut |text: &str| {
        Ok::<_, std::convert::Infallible>(scrub_error_text(text))
    });
}
