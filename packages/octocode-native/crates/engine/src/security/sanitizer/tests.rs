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
    let result =
        sanitize_content("token: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("sanitized");
    assert!(result.has_secrets);
    assert_eq!(result.warnings, vec!["1 secret(s) redacted"]);
}
