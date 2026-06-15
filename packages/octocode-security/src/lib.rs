mod detector;
mod patterns;
mod sanitizer;
mod types;

use napi_derive::napi;
use types::SanitizationResult;

// ---------------------------------------------------------------------------
// sanitizeContent — detect and redact secrets in a string
// ---------------------------------------------------------------------------

/// Detect and redact all secrets from `content`.
/// Returns the sanitized string with REDACTED placeholders, plus metadata.
///
/// Mirrors ContentSanitizer.sanitizeContent() from octocode-security-utils.
#[napi(js_name = "sanitizeContent")]
pub fn sanitize_content(content: String, file_path: Option<String>) -> SanitizationResult {
    sanitizer::sanitize_content(&content, file_path.as_deref())
}

// ---------------------------------------------------------------------------
// maskSensitiveData — mask (not redact) secrets with * every other char
// ---------------------------------------------------------------------------

/// Mask secrets in place: every even character of a matched secret is
/// replaced with `*`, preserving partial readability.
///
/// Mirrors maskSensitiveData() from octocode-security-utils.
#[napi(js_name = "maskSensitiveData")]
pub fn mask_sensitive_data(text: String) -> String {
    detector::mask_text(text)
}

// ---------------------------------------------------------------------------
// patternCount — utility exposed for testing / benchmarking
// ---------------------------------------------------------------------------

/// Returns the number of loaded patterns.
#[napi(js_name = "patternCount")]
pub fn pattern_count() -> u32 {
    patterns::PATTERNS.len() as u32
}
