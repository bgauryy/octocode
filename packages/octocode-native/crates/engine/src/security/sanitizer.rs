use super::detector;
use super::detector::{CONTENT_SIZE_LIMIT_PLACEHOLDER, MAX_CONTENT_SIZE, Scope};
use super::types::SanitizationResult;

pub fn sanitize_content(
    content: &str,
    file_path: Option<&str>,
) -> crate::error::Result<SanitizationResult> {
    sanitize_in(content, file_path.map_or(Scope::Pathless, Scope::File))
}

/// One string leaf of a tool result about the file at `source_path` (see
/// [`Scope::Leaf`]; `None`: no known file, the pathless net): its redacted
/// text, or `None` when it holds nothing to redact, so clean text is never
/// copied.
pub fn sanitize_leaf(
    content: &str,
    source_path: Option<&str>,
) -> crate::error::Result<Option<String>> {
    let scope = source_path.map_or(Scope::Pathless, Scope::Leaf);
    guarded(|| {
        if content.len() > MAX_CONTENT_SIZE {
            return Some(CONTENT_SIZE_LIMIT_PLACEHOLDER.to_owned());
        }
        detector::redact(content, scope).map(|result| result.sanitized)
    })
}

fn sanitize_in(content: &str, scope: Scope<'_>) -> crate::error::Result<SanitizationResult> {
    guarded(|| {
        // Redact oversized content before running any pattern detector.
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
        let Some(result) = detector::redact(content, scope) else {
            return SanitizationResult {
                content: content.to_owned(),
                has_secrets: false,
                secrets_detected: Vec::new(),
                warnings: Vec::new(),
            };
        };
        SanitizationResult {
            content: result.sanitized,
            has_secrets: true,
            warnings: vec![format!(
                "{} secret(s) redacted",
                result.secrets_detected.len()
            )],
            secrets_detected: result.secrets_detected,
        }
    })
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
    fn sanitize_content_redacts_oversized_content() {
        let content = "a".repeat(MAX_CONTENT_SIZE + 1);
        let result = sanitize_content(&content, None).expect("sanitized");

        assert_eq!(result.content, "[CONTENT-REDACTED-SIZE-LIMIT]");
        assert!(result.has_secrets);
        assert_eq!(result.secrets_detected, vec!["content-size-exceeded"]);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn sanitize_content_returns_no_warnings_for_clean_content() {
        let result = sanitize_content("plain text", None).expect("sanitized");

        assert_eq!(result.content, "plain text");
        assert!(!result.has_secrets);
        assert!(result.secrets_detected.is_empty());
        assert!(result.warnings.is_empty());
    }

    /// An assignment keeps its identifier and quotes: only the value is a
    /// secret, and redacted source must still parse as the same statement.
    #[test]
    fn env_var_secret_redaction_keeps_the_identifier() {
        let result = sanitize_content(
            "export const API_TOKEN = \"abcdefghijklmnopqrstuvwx\";\n",
            None,
        )
        .expect("sanitized");
        assert_eq!(
            result.content,
            "export const API_TOKEN = \"[REDACTED-ENVVARSECRETS]\";\n"
        );
        assert_eq!(result.secrets_detected, vec!["envVarSecrets"]);
    }

    /// A leaf is copied only when something in it is redacted: clean text
    /// comes back as `None`, a secret as the text `sanitize_content` makes.
    #[test]
    fn sanitize_leaf_returns_text_only_when_redacted() {
        assert_eq!(sanitize_leaf("plain text", None).expect("scanned"), None);
        assert_eq!(
            sanitize_leaf("line", Some("src/a.rs")).expect("scanned"),
            None
        );
        let token = format!("token: ghp_{}", "a".repeat(37));
        let redacted = sanitize_leaf(&token, None).expect("scanned");
        assert_eq!(
            redacted.as_deref(),
            Some(
                sanitize_content(&token, None)
                    .expect("sanitized")
                    .content
                    .as_str()
            )
        );
        assert!(!redacted.expect("redacted").contains("ghp_"));
        let oversized = "a".repeat(MAX_CONTENT_SIZE + 1);
        assert_eq!(
            sanitize_leaf(&oversized, None).expect("scanned").as_deref(),
            Some(CONTENT_SIZE_LIMIT_PLACEHOLDER)
        );
    }

    /// A leaf of a source file is scanned as that language: a YAML-style
    /// member stays, a credential beside it does not.
    #[test]
    fn sanitize_leaf_scopes_by_source_path() {
        let member = "  password: hunter2hunter2hunter2;";
        assert_eq!(
            sanitize_leaf(member, Some("src/a.ts")).expect("scanned"),
            None
        );
        assert!(
            sanitize_leaf(member, Some("config/app.yaml"))
                .expect("scanned")
                .is_some()
        );
        assert!(sanitize_leaf(member, None).expect("scanned").is_some());
    }

    #[test]
    fn sanitize_content_adds_warning_when_secret_is_redacted() {
        let result = sanitize_content("token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", None)
            .expect("sanitized");

        assert!(result.has_secrets);
        assert_eq!(result.warnings, vec!["1 secret(s) redacted"]);
    }
}
