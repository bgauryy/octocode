//! Phrases of plain words under credential-named keys.

use super::*;

/// Marks around a phrase's words (`!!ALREADY FORMATTED!!`, `<your key here>`)
/// leave it a phrase. Base64's and base64url's own characters are not marks,
/// so an encoded token stays a credential however it ends, and a marked
/// phrase under a name a person may choose a phrase for stays redacted.
#[test]
fn credential_assignments_keep_plain_word_phrases_with_marks_around_their_words() {
    for content in [
        "    key: \"!!ALREADY FORMATTED!!\",\n",
        "const apiKey = \"<your API key here>\";\n",
        "  access_token: \"(replace with your token)...\"\n",
        "  \"credential\": \"**sample credential value**\"\n",
    ] {
        let result = detect(content, Some("src/model.rs"));
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    for (content, secret) in [
        (
            "    password: \"!!correct horse battery!!\",\n",
            "correct horse",
        ),
        (
            "    client_secret: \"<open sesame door>\",\n",
            "open sesame",
        ),
        ("    key: \"Bearer abcdefghijkl==\",\n", "abcdefghijkl"),
        ("    key: \"Basic abcdefghijkl-_\",\n", "abcdefghijkl"),
        (
            "    key: \"!!a1B2c3D4e5F6 g7H8i9J0k1L2!!\",\n",
            "a1B2c3D4e5F6",
        ),
        ("    token: \"!!abcdefghijklmnop!!\",\n", "abcdefghijklmnop"),
        ("    token: \"!!!!! ????? *****\",\n", "!!!!! ?????"),
    ] {
        let result = detect(content, Some("src/model.rs"));
        assert!(!result.sanitized.contains(secret), "leaked: {content:?}");
        assert!(!result.secrets_detected.is_empty(), "{content:?}");
    }
}

/// A phrase of plain words under a `key`, `token` or `credential` name is a
/// label, placeholder or test value (rmcp's `"meta_form_key_1": "meta form
/// value 1"`): API keys and tokens carry no spaced dictionary words, so it
/// stays verbatim in every assignment form. A phrase under a `password`,
/// `passphrase` or `secret` name may be the credential itself, and a
/// phrase with an opaque piece (`Bearer <token>`) or an unspaced token
/// under any name is still redacted.
#[test]
fn credential_assignments_keep_plain_word_phrases_under_opaque_string_keys() {
    for content in [
        "    \"meta_form_key_1\": \"meta form value 1\",\n",
        "const apiKey = \"Enter your API key here.\";\n",
        "  access_token: \"replace with your access token\"\n",
        "  clientCredential: 'Paste the credential from the console'\n",
    ] {
        let result = detect(content, Some("src/model.rs"));
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let token = "a1B2c3D4e5F6g7H8i9J0k1L2";
    for (content, secret) in [
        (
            "  \"password\": \"correct horse battery staple\"\n".to_owned(),
            "correct horse battery staple",
        ),
        (
            "const dbPassphrase = \"purple monkey dishwasher lamp\";\n".to_owned(),
            "purple monkey dishwasher lamp",
        ),
        (
            "  client_secret: \"open sesame little door\"\n".to_owned(),
            "open sesame little door",
        ),
        (format!("  auth_token: \"Bearer {token}\"\n"), token),
        (format!("  \"meta_form_key_1\": \"{token}\"\n"), token),
        (
            "  api_key: \"meta form value 123456789\"\n".to_owned(),
            "meta form value 123456789",
        ),
    ] {
        let result = detect(&content, Some("src/model.rs"));
        assert!(!result.sanitized.contains(secret), "leaked: {content:?}");
        assert!(!result.secrets_detected.is_empty(), "{content:?}");
    }
}
