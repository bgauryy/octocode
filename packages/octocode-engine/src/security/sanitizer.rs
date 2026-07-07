use super::detector;
use super::detector::{CONTENT_SIZE_LIMIT_PLACEHOLDER, MAX_CONTENT_SIZE};
use super::types::SanitizationResult;

pub(crate) fn sanitize_content(content: &str, file_path: Option<&str>) -> SanitizationResult {
    // Byte-based cap (the JS fallback uses Buffer.byteLength) so the same content
    // is redacted identically whether or not the native addon is available.
    if content.len() > MAX_CONTENT_SIZE {
        return SanitizationResult {
            content: CONTENT_SIZE_LIMIT_PLACEHOLDER.to_string(),
            has_secrets: true,
            secrets_detected: vec!["content-size-exceeded".to_string()],
            warnings: vec![format!(
                "Content exceeds {} byte limit — redacted for safety",
                MAX_CONTENT_SIZE
            )],
        };
    }

    let result = if content.len() > detector::CHUNK_SIZE {
        detector::detect_chunked(content, file_path)
    } else {
        detector::detect_single(content, file_path)
    };

    let warnings = if result.secrets_detected.is_empty() {
        vec![]
    } else {
        vec![format!(
            "{} secret(s) redacted",
            result.secrets_detected.len()
        )]
    };

    SanitizationResult {
        content: result.sanitized,
        has_secrets: !result.secrets_detected.is_empty(),
        secrets_detected: result.secrets_detected,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_content_redacts_oversized_content() {
        let content = "a".repeat(MAX_CONTENT_SIZE + 1);
        let result = sanitize_content(&content, None);

        assert_eq!(result.content, "[CONTENT-REDACTED-SIZE-LIMIT]");
        assert!(result.has_secrets);
        assert_eq!(result.secrets_detected, vec!["content-size-exceeded"]);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn sanitize_content_returns_no_warnings_for_clean_content() {
        let result = sanitize_content("plain text", None);

        assert_eq!(result.content, "plain text");
        assert!(!result.has_secrets);
        assert!(result.secrets_detected.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn sanitize_content_adds_warning_when_secret_is_redacted() {
        let result = sanitize_content("token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", None);

        assert!(result.has_secrets);
        assert_eq!(result.warnings, vec!["1 secret(s) redacted"]);
    }
}
