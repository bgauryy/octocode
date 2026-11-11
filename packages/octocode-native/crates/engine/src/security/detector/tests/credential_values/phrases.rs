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

/// Prose under an opaque-string key stays prose when a word of it carries
/// ordinary punctuation inside: an abbreviation (`e.g.`), a dotted name
/// (`devDependencies.vite`), a quoted or bracketed example. Its words are
/// the runs of letters between those marks, each judged as a word.
#[test]
fn credential_assignments_keep_prose_with_punctuation_inside_its_words() {
    for content in [
        "{\"matchKeys\":\"each hit line key path, e.g. the name\"}\n",
        "{\"matchKeys\":\"each hit line key path such as devDependencies.vite here\"}\n",
        "{\"matchKeys\":\"JSON or TOML file: each hit line's key path, e.g. {\\\"221\\\":\\\"devDependencies.vite\\\"}; [i] is an array item.\"}\n",
        "{\"matchKeys\":\"JSON or TOML file: each hit line's key path, in line order, e.g. [{\\\"line\\\":289,\\\"keyPath\\\":\\\"devDependencies.typescript\\\"}].\"}\n",
        "  sort_keys: \"ascending (a.k.a. `natural`) order; see the read.me notes\"\n",
        "const tokenHint = \"Listens on 127.0.0.1:8080 by default, i.e. loopback only.\";\n",
    ] {
        for path in [None, Some("fixtures/row.json"), Some("src/model.rs")] {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
            assert!(result.secrets_detected.is_empty(), "{path:?}: {content:?}");
        }
    }
}

/// Punctuation inside a piece makes no word of an opaque run: a token beside
/// words, dotted or quoted, a run in a token alphabet's own characters
/// (`+/=`, `_`), a long number, a run with a digit or a password's
/// substitution marks stay redacted, as does any phrase under a name a
/// person may choose a phrase for.
#[test]
fn credential_assignments_still_redact_opaque_runs_inside_punctuated_phrases() {
    let token = "a1B2c3D4e5F6g7H8i9J0k1L2";
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r";
    for (content, secret) in [
        (
            format!("{{\"matchKeys\":\"use the key, e.g. {token}\"}}\n"),
            token,
        ),
        (
            format!("{{\"matchKeys\":\"each hit line key path such as {token}.vite here\"}}\n"),
            token,
        ),
        (
            format!("{{\"accessToken\":\"e.g. {{\\\"221\\\":\\\"{token}\\\"}}; is an item.\"}}\n"),
            token,
        ),
        (format!("  auth_token: \"Bearer {jwt}\"\n"), jwt),
        (
            "  api_key: \"see the value, e.g. 123456789.0\"\n".to_owned(),
            "123456789",
        ),
        (
            "  api_key: \"the name, e.g. abcd/efgh+ijkl==\"\n".to_owned(),
            "efgh+ijkl",
        ),
        (
            "  api_key: \"the name, e.g. abcdefgh_ijklmnop\"\n".to_owned(),
            "abcdefgh_ijklmnop",
        ),
        (
            "  api_key: \"my$ecret pa$$word, e.g. this\"\n".to_owned(),
            "my$ecret pa$$word",
        ),
        (
            "  \"password\": \"correct horse, e.g. battery.staple\"\n".to_owned(),
            "battery.staple",
        ),
        (
            "{\"clientSecret\":\"each hit line key path, e.g. the name\"}\n".to_owned(),
            "each hit line key path",
        ),
    ] {
        for path in [None, Some("fixtures/row.json")] {
            let result = detect(&content, path);
            assert!(
                !result.sanitized.contains(secret),
                "{path:?} leaked: {content:?}"
            );
            assert!(!result.secrets_detected.is_empty(), "{path:?}: {content:?}");
        }
    }
}
