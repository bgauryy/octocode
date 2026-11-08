//! Lines of source code that a credential rule reached but that hold no
//! credential (B165 round 8): each class stays readable, and its twin with a
//! real-shaped value stays redacted. Every value is fake.

use super::config_entries::{hides, keeps};
use super::*;

const TOKEN: &str = "q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc";

/// Each of `cases` read as the file at its path, in a read and in storage.
fn kept_in_files(cases: &[(&str, &str)]) {
    for (path, content) in cases {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            keeps(content, scope);
        }
    }
}

fn hidden_in_files(cases: &[(&str, &str)], secrets: &[&str]) {
    for (path, content) in cases {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, secrets);
        }
    }
}

/// A literal that interpolates an expression is computed by the program: its
/// text spells no credential, however long.
#[test]
fn an_interpolated_template_literal_is_readable() {
    kept_in_files(&[
        (
            "a.ts",
            "keys.map(key => `            Self::${variant(key)} => ${body(key)},`);\n",
        ),
        (
            "a.ts",
            "send({ reasoning: 'Hand off', key: `complete:${input.message}`, wake: 'action' });\n",
        ),
        ("a.ts", "const token = `ghp_${'a'.repeat(37)}`;\n"),
    ]);
    hidden_in_files(&[("a.ts", &format!("const key = `{TOKEN}`;\n"))], &[TOKEN]);
}

/// A constant's name, a namespaced name and a sentence are what a constant
/// under a credential-named declaration holds when it holds no credential.
#[test]
fn names_and_sentences_in_typed_constants_are_readable() {
    kept_in_files(&[
        ("a.rs", "const TOKEN: &str = \"OCTOCODE_ASKPASS_TOKEN\";\n"),
        ("a.rs", "const KEY: &str = \"OCTOCODE_GITHUB_CLIENT_ID\";\n"),
        (
            "a.rs",
            "const CACHE_PRIMING_TOKEN: &str = \"rustAnalyzer/cachePriming\";\n",
        ),
        (
            "a.rs",
            "const REQUEST_TOKEN_AUTH_HINT: &str = \"GitHub rejected the token supplied with this request; re-authenticate with the host and retry.\";\n",
        ),
        (
            "a.rs",
            "const KEY_BLOCK_NOTE: &str =\n    \"Redacted private-key block(s) found in the source before selecting the window.\";\n",
        ),
    ]);
    hidden_in_files(
        &[
            (
                "a.rs",
                "const TOKEN: &str = \"Q8ZR4LMT2VXW9KPN7SYD3HFJ\";\n",
            ),
            (
                "a.rs",
                "const TOKEN: &str = \"q8Zr4LmT2vXw/9KpN7sYd3HfJ\";\n",
            ),
        ],
        &["Q8ZR4LMT2VXW9KPN7SYD3HFJ", "q8Zr4LmT2vXw/9KpN7sYd3HfJ"],
    );
}

/// A value of uppercase words joined by `_` (`INIT_OPTION_SECRET`) is a
/// constant's or a placeholder's name. Its twins with digits, mixed case or
/// an access key's shape stay credentials.
#[test]
fn an_uppercase_name_is_readable_and_its_twins_are_not() {
    kept_in_files(&[(
        "a.rs",
        "let o = json!({\"token\": \"INIT_OPTION_SECRET\"});\n",
    )]);
    hidden_in_files(
        &[
            (
                "a.rs",
                "let o = json!({\"token\": \"AKIAQ8ZR4LMT2VXW9KPN\"});\n",
            ),
            (
                "a.rs",
                "let o = json!({\"token\": \"INIT8OPTION4SECRET2Q7ZK\"});\n",
            ),
            (
                "a.rs",
                "let o = json!({\"token\": \"Init_Opt9_Sec4_Q7zKw2\"});\n",
            ),
        ],
        &[
            "AKIAQ8ZR4LMT2VXW9KPN",
            "INIT8OPTION4SECRET2Q7ZK",
            "Init_Opt9_Sec4_Q7zKw2",
        ],
    );
}

/// Capital words joined by `_` under a key that names a chosen credential
/// (`password`, `passphrase`, `secret`) are a chosen password
/// (`CORRECT_HORSE_BATTERY_STAPLE`); under a key naming an opaque string they
/// stay a constant's name (B165 round 10).
#[test]
fn an_uppercase_value_under_a_chosen_credential_key_is_hidden() {
    hidden_in_files(
        &[
            (
                "a.ts",
                "const config = { password: \"CORRECT_HORSE_BATTERY_STAPLE\" };\n",
            ),
            (
                "a.ts",
                "export const DB_PASSWORD = \"CORRECT_HORSE_BATTERY_STAPLE\";\n",
            ),
            (
                "a.json",
                "{\"db_password\": \"CORRECT_HORSE_BATTERY_STAPLE\", \"passphrase\": \"PURPLE_MONKEY_DISHWASHER\"}\n",
            ),
            (
                "a.rs",
                "const SECRET: &str = \"CORRECT_HORSE_BATTERY_STAPLE\";\n",
            ),
        ],
        &["CORRECT_HORSE_BATTERY_STAPLE", "PURPLE_MONKEY_DISHWASHER"],
    );
    kept_in_files(&[
        (
            "a.rs",
            "let o = json!({\"token\": \"INIT_OPTION_SECRET\"});\n",
        ),
        ("a.rs", "const KEY: &str = \"OCTOCODE_GITHUB_CLIENT_ID\";\n"),
    ]);
}

/// A label one of whose words is a credential word (`unrelated-secret`,
/// `highly-sensitive-token`) names a credential, so it is one; a label of
/// other words under an opaque-string key stays readable (B165 round 10).
#[test]
fn a_label_holding_a_credential_word_is_hidden() {
    hidden_in_files(
        &[
            ("a.rs", "let token = \"highly-sensitive-token\";\n"),
            (
                "a.rs",
                "let o = json!({\"apiKey\":\"unrelated-secret\"});\n",
            ),
            (
                "a.rs",
                "let o = json!({\"githubToken\":\"different-request-token\"});\n",
            ),
        ],
        &[
            "highly-sensitive-token",
            "unrelated-secret",
            "different-request-token",
        ],
    );
    kept_in_files(&[
        (
            "a.ts",
            "const request = { to: b, key: 'release-question', wake: 'action' };\n",
        ),
        (
            "a.rs",
            "fn f() {\n    t.refresh_token = Some(\"synthetic-refresh\".into());\n}\n",
        ),
    ]);
}

/// Control characters framing a literal (`"\0octocode/serverStatus"`) are
/// no credential's bytes: the value between them is judged. A token so
/// framed stays a credential.
#[test]
fn a_name_framed_by_control_escapes_is_readable() {
    kept_in_files(&[
        (
            "a.rs",
            "const SERVER_STATUS_TOKEN: &str = \"\\0octocode/serverStatus\";\n",
        ),
        (
            "a.rs",
            "const KEY: &str = \"OCTOCODE_GITHUB_CLIENT_ID\\n\";\n",
        ),
    ]);
    hidden_in_files(
        &[
            (
                "a.rs",
                "const TOKEN: &str = \"\\0q8Zr4LmT2vXw9KpN7sYd3HfJ\";\n",
            ),
            (
                "a.rs",
                "const TOKEN: &str = \"\\\\0q8Zr4LmT2vXw9KpN7sYd3HfK\";\n",
            ),
        ],
        &["q8Zr4LmT2vXw9KpN7sYd3HfJ", "q8Zr4LmT2vXw9KpN7sYd3HfK"],
    );
}

/// A value a rule ends at an escaped quote (`Password=Pw0rd9q\"` in a
/// literal) ends inside the escape: its redaction takes the whole escape, so
/// the literal stays one literal. A later rule then reads the file's own
/// syntax, and a literal after it (`"jwt_secret = \""`) is no config line.
#[test]
fn a_secret_ending_at_an_escaped_quote_keeps_its_literal_whole() {
    let content = concat!(
        "fn f() {\n",
        "    let a = [\"const c = \\\"Server=x;User Id=u;Password=Pw0rd9q\\\";\\n\"];\n",
        "    let b = (format!(\"jwt_secret = \\\"{value}\\\";\"), \"jwt_secret = \\\"\");\n",
        "}\n",
    );
    let later = content.lines().nth(2).unwrap_or_default();
    for scope in [Scope::File("a.rs"), Scope::Stored(Some("a.rs"))] {
        let redacted = hides(content, scope, &["Pw0rd9q"]);
        assert_eq!(
            redacted.lines().nth(2),
            Some(later),
            "{scope:?}: {redacted}"
        );
        assert!(
            redacted.contains("Password=[REDACTED-"),
            "{scope:?}: {redacted}"
        );
    }
}

/// An armor line names a key block (`-----BEGIN RSA PRIVATE KEY-----`), a
/// reverse domain qualifies a name, and a format placeholder (`{value}`) is
/// filled in by the program: none is a credential.
#[test]
fn markers_domains_and_format_placeholders_are_readable() {
    kept_in_files(&[
        (
            "a.rs",
            "let t = \"const key = `-----BEGIN RSA PRIVATE KEY-----`;\";\n",
        ),
        (
            "a.ts",
            "const key = `-----BEGIN OPENSSH PRIVATE KEY-----`;\n",
        ),
        (
            "a.rs",
            "const SERVER_INFO_META_KEY: &str = \"io.modelcontextprotocol/serverInfo\";\n",
        ),
        (
            "a.rs",
            "let s = format!(r#\"<add name=\"db\" connectionString=\"Server=x;Password={value};\" />\"#);\n",
        ),
    ]);
    hidden_in_files(
        &[
            (
                "a.config",
                "<add name=\"db\" connectionString=\"Server=x;Password=Fake0Moo9Zed;\" />\n",
            ),
            (
                "a.rs",
                "const KEY: &str = \"io.q8zr4lmt2vxw/9KpN7sYd3HfJ\";\n",
            ),
        ],
        &["Fake0Moo9Zed", "io.q8zr4lmt2vxw/9KpN7sYd3HfJ"],
    );
}

/// A label of words (`new-login-refresh`) under an opaque-string key and a
/// timestamp are no credential, wrapped in a constructor call or not.
#[test]
fn labels_and_timestamps_in_wrapped_literals_are_readable() {
    kept_in_files(&[
        (
            "a.rs",
            "fn f() {\n    newer.token.refresh_token = Some(\"new-login-refresh\".into());\n}\n",
        ),
        (
            "a.rs",
            "fn f() {\n    t.refresh_token_expires_at = Some(\"2000-01-01T00:00:00Z\".into());\n}\n",
        ),
    ]);
    hidden_in_files(
        &[(
            "a.rs",
            "fn f() {\n    t.refresh_token = Some(\"new-q8zr4lmt2vxw-refresh\".into());\n}\n",
        )],
        &["new-q8zr4lmt2vxw-refresh"],
    );
}

/// A comment that opens with `name:` is prose before it is a config file,
/// wherever the name sits on the comment's line.
#[test]
fn a_sentence_after_a_name_opening_a_comment_is_readable() {
    kept_in_files(&[
        (
            "a.rs",
            "/// token: the new configuration, workspace or planner can change\nfn f() {}\n",
        ),
        (
            "a.rs",
            "fn f() {}\n/// credential: a boolean or a number (`true`, `8`, `0.5`).\nfn g() {}\n",
        ),
        (
            "a.rs",
            "fn f() {\n    // token: none, a non-name character, or a `.` that closes it\n}\n",
        ),
    ]);
    hidden_in_files(
        &[
            ("a.rs", &format!("/// token: {TOKEN}\nfn f() {{}}\n")),
            ("a.py", &format!("# password: {TOKEN}\nx = 1\n")),
        ],
        &[TOKEN],
    );
}

/// A member of code that the syntax places in text (code inside a string, or
/// a file that does not parse) ends in `;` or `,` after a type, a call or a
/// short literal: a member, not a config entry.
#[test]
fn a_code_member_in_text_of_a_source_file_is_readable() {
    kept_in_files(&[
        (
            "a.rs",
            "let source = r#\"\nclass Svc {\n  #secret: Vault;\n}\n\"#;\n",
        ),
        (
            "a.ts",
            "const x = ;\nconst host = {\n  token: vi.fn(),\n  accessToken: 'token',\n};\n",
        ),
        (
            "a.ts",
            "const x = ;\nexport interface A {\n  key: string;\n  credential: boolean;\n}\n",
        ),
    ]);
    hidden_in_files(
        &[
            (
                "a.ts",
                &format!("const x = ;\nconst host = {{\n  token: '{TOKEN}',\n}};\n"),
            ),
            ("a.rs", "let source = r#\"\npassword: Fake0Moo9Zed;\n\"#;\n"),
        ],
        &[TOKEN, "Fake0Moo9Zed"],
    );
}
