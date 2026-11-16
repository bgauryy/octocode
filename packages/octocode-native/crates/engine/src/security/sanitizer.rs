use super::detector::{self, ValueForm};
use super::types::SanitizationResult;

/// `content` with every known-format secret replaced by its placeholder
/// ([`detector::secret_spans`]), borrowed unchanged when it holds none.
pub fn sanitize_content(content: &str) -> crate::error::Result<SanitizationResult<'_>> {
    sanitize_content_in(content, ValueForm::Code)
}

/// [`sanitize_content`] of a source whose values are written as `form` says.
pub fn sanitize_content_in(
    content: &str,
    form: ValueForm,
) -> crate::error::Result<SanitizationResult<'_>> {
    guarded(|| {
        let Some(result) = detector::redact(content, form) else {
            return SanitizationResult {
                content: content.into(),
                has_secrets: false,
                secrets_detected: Vec::new(),
                warnings: Vec::new(),
                edits: super::Edits::default(),
            };
        };
        SanitizationResult {
            content: result.sanitized.into(),
            has_secrets: true,
            warnings: vec![format!(
                "{} secret(s) redacted",
                result.secrets_detected.len()
            )],
            secrets_detected: result.secrets_detected,
            edits: result.edits,
        }
    })
}

/// One string leaf of a tool result: its redacted text, or `None` when it
/// holds nothing to redact, so clean text is never copied.
pub fn sanitize_leaf(content: &str) -> crate::error::Result<Option<String>> {
    guarded(|| detector::redact(content, ValueForm::Code).map(|result| result.sanitized))
}

/// [`sanitize_leaf`] with what it replaced of `content`, by byte position,
/// for a reader that decides by place which replacements stand.
pub fn sanitize_leaf_edits(content: &str) -> crate::error::Result<Option<(String, super::Edits)>> {
    guarded(|| {
        detector::redact(content, ValueForm::Code).map(|result| (result.sanitized, result.edits))
    })
}

/// Whether a string of a request holds a credential: whatever a read would
/// hide, as written or as a regex spells it (see
/// [`detector::holds_credential_as_request`]).
pub fn request_holds_credential(text: &str) -> crate::error::Result<bool> {
    guarded(|| detector::holds_credential_as_request(text))
}

/// The literal a regex spelling of a request string matches
/// ([`detector::literal_form`]); `None` when it is its own literal.
pub fn request_literal_form(text: &str) -> Option<String> {
    detector::literal_form(text)
}

/// `scan`, failing closed when a detector panics on pathological input.
fn guarded<T>(scan: impl FnOnce() -> T + std::panic::UnwindSafe) -> crate::error::Result<T> {
    std::panic::catch_unwind(scan)
        .map_err(|_| crate::error::Error::new("content sanitization failed on pathological input"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_source_is_borrowed_without_copying_bytes() {
        let content = "ordinary source line\n".repeat(4096);
        let result = sanitize_content(&content).expect("scanned");
        assert_eq!(result.content.as_ptr(), content.as_ptr());
        assert_eq!(result.content, content);
        assert!(!result.has_secrets);
        assert!(result.warnings.is_empty());
    }

    /// A leaf is copied only when something in it is redacted: clean text
    /// comes back as `None`, a secret as the text `sanitize_content` makes.
    #[test]
    fn sanitize_leaf_returns_text_only_when_redacted() {
        assert_eq!(sanitize_leaf("plain text").expect("scanned"), None);
        let token = format!("token: ghp_{}", "a".repeat(37));
        let redacted = sanitize_leaf(&token).expect("scanned");
        assert_eq!(
            redacted.as_deref(),
            Some(
                sanitize_content(&token)
                    .expect("sanitized")
                    .content
                    .as_ref()
            )
        );
        assert!(!redacted.expect("redacted").contains("ghp_"));
    }

    #[test]
    fn sanitize_content_warns_once_per_pattern_redacted() {
        let result = sanitize_content("token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("sanitized");
        assert!(result.has_secrets);
        assert_eq!(result.warnings, vec!["1 secret(s) redacted"]);
    }
}
