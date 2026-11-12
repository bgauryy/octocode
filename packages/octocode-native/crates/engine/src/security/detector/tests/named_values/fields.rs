//! Where a value ends among the fields around it: an auth header keeps its
//! name and scheme, a literal's escape starts the next field, a sentence
//! states a value after `is`, and a text already redacted is redacted no
//! further when a later pass reads it again.

use super::*;

/// An auth header loses its credential only: the header's name, the `: `,
/// the scheme and what follows stay, in any case, and base64's `=` padding
/// goes with the value it pads.
#[test]
fn an_auth_header_loses_only_its_credential() {
    let padded_token = format!("{TOKEN}==");
    let rows: Vec<(String, &str)> = vec![
        (format!("Authorization: Bearer {TOKEN}\n"), TOKEN),
        (format!("authorization: bearer {TOKEN}\n"), TOKEN),
        (format!("AUTHORIZATION: BEARER {TOKEN}\n"), TOKEN),
        (format!("Proxy-Authorization: Bearer {TOKEN}\n"), TOKEN),
        (
            format!("Authorization: Bearer {padded_token}\n"),
            &padded_token,
        ),
        (
            format!("see Authorization: Bearer {TOKEN}. Then retry.\n"),
            TOKEN,
        ),
        (format!("Authorization: Basic {PADDED}\n"), PADDED),
        (format!("Proxy-Authorization: Basic {PADDED}\n"), PADDED),
        (
            "Authorization: Basic dXNlcjpGYWJyaWNhdGVkUGFzcz+/==\n".to_owned(),
            "dXNlcjpGYWJyaWNhdGVkUGFzcz+/==",
        ),
        (format!("authorization: basic {ENCODED}\n"), ENCODED),
    ];
    for (content, secret) in &rows {
        for path in [None, Some("docs/issue.md"), Some("src/req.js")] {
            assert_redacts_only(content, path, secret);
        }
    }
    // A scheme with no credential of the header's length after it is prose.
    stays(&[
        ("Authorization: Bearer <token>\n", "docs/api.md"),
        ("Authorization: Basic abc\n", "docs/api.md"),
        (
            "the Authorization header carries a Bearer token\n",
            "docs/api.md",
        ),
    ]);
}

/// A control-character escape inside a literal (`\r\n`, `\t`, `\u000a`)
/// ends an opaque value whose literal goes on with the next field's words:
/// that text stays, for the quoted-literal rule as for the named-value rule.
#[test]
fn an_escape_before_the_next_field_ends_the_value() {
    let long = format!("{TOKEN}{OTHER}");
    for escape in ["\\r\\n", "\\n", "\\t", "\\u000a", "\\x0a"] {
        for content in [
            format!("const apiKey = \"{long}{escape}Host: example.com\";\n"),
            format!("API_KEY = \"{long}{escape}Accept: */*\"\n"),
        ] {
            for path in [None, Some("src/req.js"), Some("docs/issue.md")] {
                assert_redacts_only(&content, path, &long);
            }
        }
    }
}

/// A value whose literal goes on with more of it after the escape (a key's
/// base64 lines, a second token) is one value written over lines, and a
/// value before the escape that is no credential leaves the whole literal to
/// the rule's judgment: both go whole. In a raw string the backslash is the
/// value's own.
#[test]
fn an_escape_inside_one_value_ends_nothing() {
    let lines = format!("{TOKEN}\\n{OTHER}");
    let worded = format!("correct-horse-battery\\n{TOKEN}{OTHER}");
    hides_only(&[
        (
            &format!("const apiKey = \"{lines}\";\n"),
            "src/req.js",
            &[&lines],
        ),
        (
            &format!("const apiKey = \"{worded}\";\n"),
            "src/req.js",
            &[&worded],
        ),
    ]);
    let raw = format!("api_key = r\"{TOKEN}{OTHER}\\r\\nHost: example.com\"\n");
    assert_redacts_only(
        &raw,
        Some("src/req.py"),
        &format!("{TOKEN}{OTHER}\\r\\nHost: example.com"),
    );
}

/// A text a pass redacted is redacted no further by any later pass that
/// reads it, whatever scope it reads in: a placeholder framed by a line's
/// escape (`"[REDACTED-…]\r\n"`) is the tool's own text. One value keeps one
/// placeholder in every view, the text sent to a provider among them.
#[test]
fn a_redacted_text_is_redacted_no_further() {
    let content = format!(
        "const apiKey = \"{TOKEN}{OTHER}\\r\\n\";\napiKey = \"{TOKEN}{OTHER}\\r\\n\";\nauthKey = \"{TOKEN}{OTHER}\\r\\nHost: example.com\";\n// Authorization: Basic {PADDED}\n// the password is {PASSWORD}\n"
    );
    for path in [
        None,
        Some("src/req.js"),
        Some("docs/issue.md"),
        Some("conf/app.env"),
    ] {
        let shown = detect(&content, path).sanitized;
        assert!(shown.contains("\\r\\n\";"), "{path:?}: {shown:?}");
        for scope in [Scope::Stored(None), Scope::Pathless] {
            assert_eq!(detect_in(&shown, scope).sanitized, shown, "{path:?}");
        }
    }
}

/// A sentence that states a credential's value after `is` loses the value
/// when it is opaque, of any length and any characters; the words of prose
/// after the same names stay.
#[test]
fn a_value_a_sentence_states_is_hidden_and_prose_stays() {
    let short = "Hunter2Xq9vB7mK";
    let symbols = "Tr0ub4dor&3xK9!zQ";
    let rows: Vec<(String, &str)> = vec![
        (format!("the password is {symbols}\n"), symbols),
        (format!("the password is {short}\n"), short),
        (format!("The db_password is \"{short}\".\n"), short),
        (
            format!("Note: the API token IS {short}, then stop.\n"),
            short,
        ),
    ];
    for (content, secret) in &rows {
        for path in [None, Some("docs/setup.md"), Some("notes.txt")] {
            assert_redacts_only(content, path, secret);
        }
    }
    let prose = [
        "the password is required\n",
        "the token is expired\n",
        "the secret is stored in vault\n",
        "the password is hunter2\n",
        "the token is refreshed by refreshAccessToken()\n",
        "the secret is AWS_SECRET_ACCESS_KEY\n",
        "the key is computeCacheKeyForRequest(request)\n",
        "the token is `SessionToken`\n",
        "the password is <your-password>\n",
        "the password is ${DB_PASSWORD}\n",
        "the token is 3 characters long\n",
        "the public key is ssh-ed25519\n",
        "why the token is HTTP-only\n",
    ];
    stays(&prose.map(|content| (content, "docs/setup.md")));
}

/// Under a field of the `Sec-` namespace only the nonce the user agent fills
/// stays (base64 of 16 bytes: 22 characters and `==`); any other value there
/// is held as under any credential's name.
#[test]
fn a_sec_field_keeps_only_its_nonce() {
    stays(&[
        (
            "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\n",
            "docs/ws.md",
        ),
        (
            "sec-websocket-key: x3JJHMbDL1EzLkh9GBhXDw==\n",
            "docs/ws.md",
        ),
    ]);
    let long = format!("{TOKEN}{OTHER}");
    let not_a_nonce = format!("Sec-WebSocket-Key: {long}\n");
    let unpadded = format!("Sec-WebSocket-Key: {TOKEN}\n");
    hides_only(&[
        (&not_a_nonce, "docs/ws.md", &[&long]),
        (&unpadded, "docs/ws.md", &[TOKEN]),
    ]);
}
