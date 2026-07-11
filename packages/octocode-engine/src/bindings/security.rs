use napi::{Error, Result, Status};
use napi_derive::napi;

/// Detect and redact all secrets from `content`, returning the sanitized string
/// with `[REDACTED-*]` placeholders plus detection metadata. `file_path` gates
/// file-context patterns (e.g. Kubernetes/`.env` secrets).
///
/// The scan is wrapped in `catch_unwind` — same guard as binary/signatures/
/// structural — so a panic on pathological input becomes a catchable JS error
/// instead of aborting the Node process, and never silently falls through to
/// returning unredacted content.
#[napi(js_name = "sanitizeContent")]
pub fn sanitize_content(
    content: String,
    file_path: Option<String>,
) -> Result<crate::security::types::SanitizationResult> {
    std::panic::catch_unwind(|| {
        crate::security::sanitizer::sanitize_content(&content, file_path.as_deref())
    })
    .map_err(|_| {
        Error::new(
            Status::InvalidArg,
            "content sanitization failed on pathological input".to_string(),
        )
    })
}

/// Mask secrets in place: every even-indexed char of a matched secret becomes
/// `*`, preserving partial readability. File-context patterns are skipped.
#[napi(js_name = "maskSensitiveData")]
pub fn mask_sensitive_data(text: String) -> String {
    crate::security::detector::mask_text(text)
}

/// Number of loaded secret-detection patterns (testing / benchmarking).
#[napi(js_name = "patternCount")]
pub fn pattern_count() -> u32 {
    crate::security::patterns::PATTERNS.len() as u32
}
