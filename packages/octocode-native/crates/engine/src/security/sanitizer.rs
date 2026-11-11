use super::detector;
use super::detector::Scope;
use super::types::SanitizationResult;

/// Redact JSON member values whose surrounding object identifies a secret.
/// Raw text leaves still need their normal source-aware scan. Returns the
/// removed values so a caller can also hide copies elsewhere in the data.
pub fn sanitize_structure(
    value: &mut serde_json::Value,
) -> crate::error::Result<Vec<(String, String)>> {
    let hidden = guarded(|| detector::structured::matches(value))??;
    Ok(detector::structured::apply(value, hidden))
}

pub fn sanitize_content<'a>(
    content: &'a str,
    file_path: Option<&str>,
) -> crate::error::Result<SanitizationResult<'a>> {
    sanitize_content_within(content, file_path, super::MAX_CONTENT_SIZE)
}

/// [`sanitize_content`] judging a text longer than `whole_text_limit` in one
/// pass over its spans, as one past [`super::MAX_CONTENT_SIZE`] always is.
pub fn sanitize_content_within<'a>(
    content: &'a str,
    file_path: Option<&str>,
    whole_text_limit: usize,
) -> crate::error::Result<SanitizationResult<'a>> {
    sanitize_in(
        content,
        file_path.map_or(Scope::Pathless, Scope::File),
        whole_text_limit,
    )
}

/// One string leaf of a tool result about the file at `source_path` (see
/// [`Scope::Leaf`]; `None`: no known file, the pathless net): its redacted
/// text, or `None` when it holds nothing to redact, so clean text is never
/// copied.
pub fn sanitize_leaf(
    content: &str,
    source_path: Option<&str>,
) -> crate::error::Result<Option<String>> {
    redact_leaf(content, Scope::of_leaf(source_path))
}

/// [`sanitize_leaf`] of a leaf whose text was cut from what
/// [`sanitize_content`] returned for the whole file at `judged_path` (see
/// [`Scope::Judged`]): only the patterns that whole pass did not apply run,
/// so a fragment is not judged again without the syntax around it.
pub fn sanitize_judged_leaf(
    content: &str,
    source_path: Option<&str>,
    judged_path: &str,
) -> crate::error::Result<Option<String>> {
    redact_leaf(
        content,
        Scope::Judged {
            leaf: source_path,
            source: judged_path,
        },
    )
}

/// [`sanitize_leaf`] of a leaf about to be kept (see [`Scope::Stored`]): what no
/// later read, of any file, would have to redact.
pub fn sanitize_stored_leaf(
    content: &str,
    source_path: Option<&str>,
) -> crate::error::Result<Option<String>> {
    redact_leaf(content, Scope::Stored(source_path))
}

/// Whether a string of a request holds a credential, judged as a search
/// term is: by its value, not by the name beside it (see
/// [`detector::holds_credential_as_request`]).
pub fn request_holds_credential(text: &str) -> crate::error::Result<bool> {
    guarded(|| detector::holds_credential_as_request(text))
}

fn redact_leaf(content: &str, scope: Scope<'_>) -> crate::error::Result<Option<String>> {
    guarded(|| detector::redact(content, scope).map(|result| result.sanitized))
}

fn sanitize_in<'a>(
    content: &'a str,
    scope: Scope<'_>,
    whole_text_limit: usize,
) -> crate::error::Result<SanitizationResult<'a>> {
    guarded(|| {
        let Some(result) = detector::redact_within(content, scope, whole_text_limit) else {
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
        let result = sanitize_content(&content, Some("src/plain.rs")).expect("scanned");
        assert_eq!(result.content.as_ptr(), content.as_ptr());
        assert_eq!(result.content, content);
        assert!(!result.has_secrets);
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
                    .as_ref()
            )
        );
        assert!(!redacted.expect("redacted").contains("ghp_"));
    }

    /// A leaf of a source file is scanned as that language: a member of a
    /// declaration stays, a bare `name: value` line (a label, how YAML pasted
    /// into code parses) and a credential beside it do not.
    #[test]
    fn sanitize_leaf_scopes_by_source_path() {
        let declared = "interface Cfg {\n  password: hunter2hunter2hunter2;\n}";
        assert_eq!(
            sanitize_leaf(declared, Some("src/a.ts")).expect("scanned"),
            None
        );
        let member = "  password: hunter2hunter2hunter2;";
        assert!(
            sanitize_leaf(member, Some("src/a.ts"))
                .expect("scanned")
                .is_some()
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
