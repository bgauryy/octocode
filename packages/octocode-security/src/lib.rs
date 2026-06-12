#![deny(clippy::all)]

mod detector;
mod patterns;

use napi_derive::napi;

const MAX_CONTENT_SIZE: usize = 10_000_000;

// ---------------------------------------------------------------------------
// Public types (mirrored from octocode-security-utils TypeScript types)
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct SanitizationResult {
    pub content: String,
    pub has_secrets: bool,
    pub secrets_detected: Vec<String>,
    pub warnings: Vec<String>,
}

#[napi(object)]
pub struct MaskResult {
    pub text: String,
    pub matched_count: u32,
}

// ---------------------------------------------------------------------------
// sanitizeContent — detect and redact secrets in a string
// ---------------------------------------------------------------------------

/// Detect and redact all secrets from `content`.
/// Returns the sanitized string with REDACTED placeholders, plus metadata.
///
/// Mirrors ContentSanitizer.sanitizeContent() from octocode-security-utils.
#[napi(js_name = "sanitizeContent")]
pub fn sanitize_content(content: String, file_path: Option<String>) -> SanitizationResult {
    if content.len() > MAX_CONTENT_SIZE {
        return SanitizationResult {
            content: "[CONTENT-REDACTED-SIZE-LIMIT]".to_string(),
            has_secrets: true,
            secrets_detected: vec!["content-size-exceeded".to_string()],
            warnings: vec![format!(
                "Content exceeds {} character limit — redacted for safety",
                MAX_CONTENT_SIZE
            )],
        };
    }

    let fp = file_path.as_deref();
    let result = if content.len() > detector::CHUNK_SIZE {
        detector::detect_chunked(&content, fp)
    } else {
        detector::detect_single(&content, fp)
    };

    let warnings = if result.secrets_detected.is_empty() {
        vec![]
    } else {
        vec![format!("{} secret(s) redacted", result.secrets_detected.len())]
    };

    SanitizationResult {
        content: result.sanitized,
        has_secrets: !result.secrets_detected.is_empty(),
        secrets_detected: result.secrets_detected,
        warnings,
    }
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
