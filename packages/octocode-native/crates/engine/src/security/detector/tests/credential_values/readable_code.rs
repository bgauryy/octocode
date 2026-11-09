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

/// A connection string's field read up to an escaped quote
/// (`Password={value}\"` in a literal) is judged as the program holds it:
/// the escape is the quote it writes, and a quote that frames the field is
/// no part of it, so a format placeholder stays readable. A real password
/// written so stays hidden (B190 item 1).
#[test]
fn a_format_placeholder_before_an_escaped_quote_is_readable() {
    kept_in_files(&[(
        "a.rs",
        "fn f() {\n    let s = format!(\"Server=x;Password={value}\\\";\");\n}\n",
    )]);
    hidden_in_files(
        &[(
            "a.rs",
            "fn f() {\n    let s = format!(\"Server=x;Password=Pw0rd9q\\\";\");\n}\n",
        )],
        &["Pw0rd9q"],
    );
}

/// Capital words joined by `_` under a key naming a chosen credential are
/// a variable's name when the same source expands that variable
/// (`"$OCTOCODE_ASKPASS_PASSWORD_PROMPT"`, `${NAME}`, `%NAME%`): a chosen
/// password is never also expanded as a variable in its own source. A name
/// written only as text again, or expanded as a longer name, stays hidden
/// (B190, R6 question 1).
#[test]
fn a_constant_name_the_source_expands_is_readable() {
    kept_in_files(&[
        (
            "a.rs",
            concat!(
                "const PASSWORD_PROMPT: &str = \"OCTOCODE_ASKPASS_PASSWORD_PROMPT\";\n",
                "const SCRIPT: &str = \"case \\\"$OCTOCODE_ASKPASS_PASSWORD_PROMPT\\\" in '') exit 1 ;; esac\\n\";\n",
            ),
        ),
        (
            "a.ts",
            "const config = { password: \"APP_DB_PASSWORD_VALUE\" };\nconst run = \"connect --pass ${APP_DB_PASSWORD_VALUE}\";\n",
        ),
        (
            "a.ts",
            "const config = { password: \"APP_DB_PASSWORD_VALUE\" };\nconst run = \"connect /pass %APP_DB_PASSWORD_VALUE%\";\n",
        ),
    ]);
    for (path, content) in [
        (
            "a.ts",
            "const config = { password: \"CORRECT_HORSE_BATTERY_STAPLE\" };\nconst copy = \"CORRECT_HORSE_BATTERY_STAPLE\";\n",
        ),
        (
            "a.rs",
            "const PASSWORD: &str = \"CORRECT_HORSE_BATTERY_STAPLE\";\nconst S: &str = \"$CORRECT_HORSE_BATTERY_STAPLE_2\";\n",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            let redacted = detect_in(content, scope).sanitized;
            let first = redacted.lines().next().unwrap_or_default();
            assert!(
                !first.contains("CORRECT_HORSE_BATTERY_STAPLE"),
                "{scope:?}: {redacted}"
            );
        }
    }
}

/// RV1 (round 2, R2): a message whose words follow a credential key and a
/// colon in a one-line literal is prose, as it is in a comment: it stays
/// readable. A credential after `key:` (one word, no space) stays hidden.
#[test]
fn rv1_prose_after_a_credential_key_in_a_one_line_literal_is_readable() {
    kept_in_files(&[
        (
            "a.rs",
            "fn f() { let m = \"password: must be at least 8 characters\"; }\n",
        ),
        (
            "a.ts",
            "throw new Error(\"password: Required field is missing\");\n",
        ),
        (
            "a.ts",
            "const d = \"client_secret: The client secret of your OAuth application\";\n",
        ),
        (
            "a.rs",
            "fn f() { assert_eq!(msg, \"secret_key: expected 32 bytes, got 16\"); }\n",
        ),
        (
            "a.ts",
            "const label = \"Password: Enter at least twelve characters please\";\n",
        ),
    ]);
    hidden_in_files(
        &[
            (
                "a.rs",
                "fn f() { let m = \"password: Zq7xK2mPlanted9xx\"; }\n",
            ),
            (
                "a.ts",
                "const d = \"a=1\\nclient_secret: Zq7xK2mPlanted9xx\";\n",
            ),
        ],
        &["Zq7xK2mPlanted9xx"],
    );
}

/// RV2 (round 2, R4): an interpolated literal (C# `$"…{x}…"`, Python
/// `f'…{x}…'`) is computed by the program, so its text spells no credential.
/// The same body without a prefix that interpolates (`b'…'`), or with no
/// hole, stays hidden.
#[test]
fn rv2_an_interpolated_literal_under_a_credential_name_is_readable() {
    kept_in_files(&[
        (
            "a.cs",
            "class A { string apiKey = $\"{baseKey}{suffixValue}\"; }\n",
        ),
        ("a.py", "api_key = f'{base}_{suffix}_value'\n"),
        ("a.py", "api_key = rf'{base}_{suffix}_value'\n"),
    ]);
    hidden_in_files(
        &[
            ("a.py", "secret_key = b'Zq7xK2mPlanted9xx'\n"),
            ("a.py", "secret_key = f'Zq7xK2mPlanted9xx'\n"),
            ("a.py", "secret_key = b'{Zq7xK2mPlanted9xx}'\n"),
            (
                "a.cs",
                "class A { string apiKey = $\"Zq7xK2mPlanted9xx\"; }\n",
            ),
        ],
        &["Zq7xK2mPlanted9xx"],
    );
}

/// RV3 (round 2, R1): many constant-name values under credential keys cost
/// what their bytes cost: the names a text expands are read once per text,
/// not once per value.
#[test]
fn rv3_constant_name_values_scan_in_linear_time() {
    let text = |n: usize| -> String {
        (0..n)
            .map(|i| format!("const PASSWORD_{i}: &str = \"CORRECT_HORSE_BATTERY_STAPLE\";\n"))
            .collect()
    };
    assert_scales_linearly(&text(500), &text(4_000), |content| {
        std::hint::black_box(redact(content, Scope::File("a.rs")));
    });
}

/// Round 2, R3: a placeholder to fill in (`<your-token-here>`,
/// `tvly-YOUR-KEY-HERE`), a versioned type name (`GithubTokenProviderV2`)
/// and a code expression of names (`resultCount===0&&Result.isErr`,
/// `ctx.accessToken/other.accessToken-1`) under a credential key in a
/// one-line literal spell no credential. Each twin with a real-shaped piece
/// stays hidden.
#[test]
fn placeholders_versioned_names_and_code_expressions_in_a_literal_are_readable() {
    kept_in_files(&[
        ("a.ts", "const s = 'TOKEN=<your-token-here>';\n"),
        ("a.ts", "const s = `AUTH_TOKEN=GithubTokenProviderV2`;\n"),
        (
            "a.ts",
            "const s = `password: resultCount===0&&Result.isErr`;\n",
        ),
        (
            "a.ts",
            "const s = `accessToken: ctx.accessToken/other.accessToken-1`;\n",
        ),
        (
            "a.ts",
            "console.log(dim('      TAVILY_API_KEY=tvly-YOUR-KEY-HERE\\n'));\n",
        ),
    ]);
    hidden_in_files(
        &[
            ("a.ts", "const s = 'TOKEN=<Zq7xK2mPlanted9xx>';\n"),
            ("a.ts", "const s = `AUTH_TOKEN=Zq7xK2mPlanted9xxV2`;\n"),
            (
                "a.ts",
                "const s = `password: resultCount===0&&Zq7xK2mPlanted9xx`;\n",
            ),
            (
                "a.ts",
                "const s = `accessToken: ctx.Zq7xK2mPlanted9xx/other.accessToken-1`;\n",
            ),
            (
                "a.ts",
                "console.log(dim('      TAVILY_API_KEY=tvly-Zq7xK2mPlanted9xx\\n'));\n",
            ),
        ],
        &["Zq7xK2mPlanted9xx"],
    );
}

/// RV4 (round 3, D1): placeholder words around a real-shaped secret do not
/// make it a placeholder: a fill-in is plain words only. Each value here
/// holds a piece with a digit or an unpronounceable run, under any key.
#[test]
fn rv4_fill_in_words_around_a_real_secret_stay_hidden() {
    for (path, content, secret) in [
        (
            "a.ts",
            "const apiKey = \"your-Zq7xK2mPlanted9-key-here\";\n",
            "Zq7xK2mPlanted9",
        ),
        (
            "a.ts",
            "const token = \"Zq7xK2mPlanted9_your_token_here\";\n",
            "Zq7xK2mPlanted9",
        ),
        (
            "a.py",
            "API_KEY = \"sk-proj-Zq7xK2mPlanted9abc your key here\"\n",
            "Zq7xK2mPlanted9abc",
        ),
        (
            "a.ts",
            "const cfg = { password: \"your-Hunter2-here\" };\n",
            "Hunter2",
        ),
        (
            "a.ts",
            "const cfg = { password: \"Your_Password_Here_Zq7xK2mPlanted9\" };\n",
            "Zq7xK2mPlanted9",
        ),
        (
            "config.yaml",
            "api_token: your_Zq7xK2mPlanted9_here\n",
            "Zq7xK2mPlanted9",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &[secret]);
        }
    }
}

/// RV5 (round 3, D2): `{{` and `}}` write braces in an interpolating
/// literal, so `f"{{…}}"` holds the text between them, not a hole.
#[test]
fn rv5_escaped_braces_in_an_interpolated_literal_are_literal_text() {
    for (path, content) in [
        ("a.py", "api_key = f\"{{Zq7xK2mPlanted9abcdef}}\"\n"),
        ("a.py", "api_key = f\"Zq7xK2mPlanted9abcdef{{}}\"\n"),
        (
            "a.cs",
            "class A { string apiKey = $\"{{Zq7xK2mPlanted9abcdef}}\"; }\n",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &["Zq7xK2mPlanted9abcdef"]);
        }
    }
    // A hole after escaped braces is still a hole.
    kept_in_files(&[("a.py", "api_key = f\"{{{base}_{suffix}_value}}\"\n")]);
}

/// RV6 (round 3): in a comment an interpolating prefix makes its hole
/// readable, and a key with no such prefix in the same comment is judged on
/// its own.
#[test]
fn rv6_a_comment_judges_the_interpolating_prefix_through_its_key_facts() {
    for (path, content) in [
        ("a.py", "# api_key = f'{base}_{suffix}_value'\n"),
        ("a.cs", "// apiKey = $\"{baseKey}{suffixValue}\";\n"),
    ] {
        // A stored leaf reads the comment as a properties line, which hides
        // any value (file-class policy), so only the read is checked.
        keeps(content, Scope::File(path));
    }
    for scope in [Scope::File("a.py"), Scope::Stored(Some("a.py"))] {
        hides(
            "# token: x, api_key = f'{base}{suffix}', secret = '{Zq7xK2mPlanted9abcdef}'\n",
            scope,
            &["Zq7xK2mPlanted9abcdef"],
        );
    }
}

/// Round 3: a version adds no evidence that a value is a name. A random token
/// in the versioned shape (`GyyiXidMolckV598`: pronounceable pieces, then
/// `V` and digits) whose name part is shorter than a credential literal stays
/// hidden, while a type name of credential length before its version
/// (`GithubTokenProviderV2`) stays readable.
#[test]
fn a_random_token_in_the_versioned_shape_stays_hidden() {
    hidden_in_files(
        &[
            ("a.ts", "const token = \"GyyiXidMolckV598\";\n"),
            ("a.ts", "const s = `AUTH_TOKEN=GyyiXidMolckV598`;\n"),
            ("config.yaml", "api_token: GyyiXidMolckV598\n"),
        ],
        &["GyyiXidMolckV598"],
    );
    kept_in_files(&[("a.ts", "const s = `AUTH_TOKEN=GithubTokenProviderV2`;\n")]);
}

/// RV7 (round 4, E1): under a key naming a chosen credential, words between
/// `your` and `here` that name no credential are the password, not a
/// placeholder; and a piece in mixed case (`HuNtEr…`) is no plain word under
/// any key.
#[test]
fn rv7_a_chosen_password_or_a_mixed_case_piece_between_fill_in_words_stays_hidden() {
    for (path, content, secret) in [
        (
            "a.ts",
            "const cfg = { password: \"your_wojabegupysahoge_here\" };\n",
            "wojabegupysahoge",
        ),
        (
            "a.ts",
            "const cfg = { password: \"your-correct-horse-battery-staple-here\" };\n",
            "correct-horse-battery-staple",
        ),
        (
            "a.ts",
            "const cfg = { password: \"your-HuNtErTwOfIsH-here\" };\n",
            "HuNtErTwOfIsH",
        ),
        (
            "a.ts",
            "const apiKey = \"your-JOSIXuZAGIBEteSa-here\";\n",
            "JOSIXuZAGIBEteSa",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &[secret]);
        }
    }
}

/// RV7 twin (round 4): placeholders that name the credential stay readable.
#[test]
fn rv7_placeholders_that_name_the_credential_stay_readable() {
    for content in [
        "const cfg = { password: \"your-password-here\" };\n",
        "const cfg = { password: \"YOUR_PASSWORD_HERE\" };\n",
        "const apiKey = \"tvly-YOUR-KEY-HERE\";\n",
        "const apiKey = \"your_api_key_here\";\n",
        "const apiKey = \"your-openai-api-key-here\";\n",
    ] {
        keeps(content, Scope::File("a.ts"));
    }
}

/// B237a: a bracketed value is a fill-in only when it says so: several words,
/// or a piece naming a credential. One long pronounceable word in brackets is
/// a secret a person wrapped in angle brackets, under any credential key.
#[test]
fn a_single_bracketed_word_under_a_credential_key_is_hidden() {
    hidden_in_files(
        &[
            ("a.yaml", "apiKey: \"<Kavotemixalupolasiru>\"\n"),
            ("a.yaml", "apiKey: \"<Kavotemixalupo>\"\n"),
            ("a.yaml", "apiKey: <Kavotemixalupo>\n"),
            ("a.py", "token = \"<Kavotemixalupolasiru>\"\n"),
            ("a.ts", "const apiKey = '<Kavotemixalupo>';\n"),
        ],
        &["Kavotemixalupo"],
    );
}

/// B237a twin: words that say what to fill in stay readable in brackets.
#[test]
fn bracketed_placeholders_that_say_what_to_fill_in_stay_readable() {
    // Stored text of a YAML name is read as a properties file, which keeps no
    // bracketed value: only the read's scope judges the placeholder.
    for content in [
        "apiKey: \"<API_KEY>\"\n",
        "apiKey: \"<your-token-here>\"\n",
        "apiKey: \"<password>\"\n",
        "prefix: \"<your-token-here>\"\n",
    ] {
        keeps(content, Scope::File("a.yaml"));
    }
    kept_in_files(&[
        ("a.py", "token = \"<your-api-key>\"\n"),
        ("a.ts", "const apiKey = '<insert-key>';\n"),
    ]);
}

/// B238: a quoted literal under a chosen credential key is a value even when
/// a comparison or logical operator joins names in it; the same names as
/// unquoted code stay readable.
#[test]
fn a_quoted_literal_joined_by_operators_under_a_chosen_key_is_hidden() {
    hidden_in_files(
        &[
            ("a.py", "password = \"adminUser==rootPassword\"\n"),
            ("a.py", "password = 'adminUser&&rootPassword'\n"),
            ("a.ts", "const password = \"adminUser||rootPassword\";\n"),
        ],
        &["adminUser", "rootPassword"],
    );
    keeps(
        "password: resultCount===0&&Result.isErr\n",
        Scope::File("a.yaml"),
    );
    kept_in_files(&[(
        "a.ts",
        "const ok = password === resultCount && Result.isErr;\n",
    )]);
}

/// B238: a literal beside an f-string hole is judged piece by piece: an opaque
/// piece is a credential, a hole with no literal beside it is code.
#[test]
fn an_opaque_literal_piece_beside_a_format_hole_is_hidden() {
    hidden_in_files(
        &[
            ("a.py", "password = f'Zq7xK2mPlanted9Zq7xK2{suffix}'\n"),
            ("a.py", "password = f'{prefix}Zq7xK2mPlanted9Zq7xK2'\n"),
            ("a.ts", "const token = `Zq7xK2mPlanted9Zq7xK2${suffix}`;\n"),
        ],
        &["Zq7xK2mPlanted9Zq7xK2"],
    );
    kept_in_files(&[
        ("a.py", "password = f'{base}_x'\n"),
        ("a.py", "password = f'{suffix}'\n"),
        ("a.ts", "const token = \"${HOME}/x\";\n"),
        ("a.ts", "const token = `pi-${id}`;\n"),
    ]);
}

/// B238: a YAML plain scalar ends at `: `, not at any colon, so a colon inside
/// a bare value is part of the secret.
#[test]
fn a_yaml_bare_value_with_a_colon_inside_is_hidden() {
    hidden_in_files(
        &[(
            "a.yaml",
            "db:\n  password: Zq7xK2:mPlanted9Zq7xK2mPlanted\n",
        )],
        &["Zq7xK2", "mPlanted9"],
    );
    kept_in_files(&[
        ("a.yaml", "db:\n  url: https://example.com/x\n"),
        ("a.yaml", "db:\n  tokenUrl: https://example.com/x\n"),
    ]);
}
