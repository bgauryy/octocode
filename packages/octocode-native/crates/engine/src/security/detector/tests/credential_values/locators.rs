//! Values that name a place in the file system, not a credential.

use super::*;

/// A path, or a command-line option assigning one, says where something is:
/// under a credential-named key it is a locator, as a `*_path` key's value is,
/// so it stays readable. A message that quotes a value after a name ending in
/// a credential word (a repository called `test_keys`) keeps it too.
#[test]
fn credential_assignments_keep_path_and_option_locators() {
    for content in [
        "Branch, tag, or SHA not found for trufflesecurity/test_keys: \"--upload-pack=/usr/bin/touch\"; the default branch is \"main\".",
        "  ssh_key: \"~/.ssh/id_ed25519_fake\"\n",
        "{\"secret_key\": \"/run/secrets/app_secret_key\"}",
        "const sshKey = \"./certs/dev-key.pem\";\n",
        "  \"token\": \"--token-file=/var/run/secrets/token\"\n",
        "  password: /Users/Shared/vault/password.txt\n",
    ] {
        let result = detect(content, None);
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, secret) in [
        (
            "{\"api_key\": \"/K7MDENG/bPxRfiCYFAKEKEYzz\"}".to_owned(),
            "K7MDENG/bPxRfiCYFAKEKEYzz".to_owned(),
        ),
        (format!("  \"token\": \"--token={value}\"\n"), value.clone()),
        (
            "  secret: \"/abc+def/ghi=jkl/mnopqrst\"\n".to_owned(),
            "abc+def/ghi=jkl".to_owned(),
        ),
    ] {
        let result = detect(&content, None);
        assert!(!result.sanitized.contains(&secret), "leaked: {content:?}");
    }
}
