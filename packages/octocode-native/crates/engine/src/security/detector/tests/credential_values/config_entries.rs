//! Credential entries of compose, Kubernetes, INI and dotenv files (B157), and
//! the code that must stay readable beside them (B156). Every value is fake.

use super::*;

/// Redacts each of `secrets` in `content` read as `scope`, keeps its line
/// breaks as written, and redacts nothing more on a second pass.
pub(super) fn hides(content: &str, scope: Scope<'_>, secrets: &[&str]) -> String {
    let result = detect_in(content, scope);
    for secret in secrets {
        assert!(
            !result.sanitized.contains(secret),
            "{scope:?}: {secret:?} leaked: {content:?} -> {:?}",
            result.sanitized
        );
    }
    assert_eq!(
        line_breaks(&result.sanitized).collect::<Vec<_>>(),
        line_breaks(content).collect::<Vec<_>>(),
        "{scope:?}: {content:?} -> {:?}",
        result.sanitized
    );
    let again = detect_in(&result.sanitized, scope);
    assert_eq!(again.sanitized, result.sanitized, "{scope:?}: not stable");
    result.sanitized
}

/// Reads `content` as `scope` and keeps it byte for byte.
pub(super) fn keeps(content: &str, scope: Scope<'_>) {
    let result = detect_in(content, scope);
    assert_eq!(result.sanitized, content, "{scope:?}");
}

/// Every kind of scope a text is judged in, with the paths of config files.
const CONFIG_SCOPES: &[Scope<'static>] = &[
    Scope::Pathless,
    Scope::File("docker-compose.yml"),
    Scope::File("k8s/deployment.yaml"),
    Scope::File("conf/app.ini"),
    Scope::File("app.properties"),
    Scope::File("conf/.env"),
    Scope::Leaf("compose.yaml"),
    Scope::Stored(None),
    Scope::Stored(Some("compose.yaml")),
];

/// The scopes that read YAML text.
const YAML_SCOPES: &[Scope<'static>] = &[
    Scope::Pathless,
    Scope::File("docker-compose.yml"),
    Scope::File("k8s/deployment.yaml"),
    Scope::Leaf("compose.yaml"),
];

/// A `- NAME=value` list entry (a compose or Kubernetes `environment:` list)
/// is a credential where its bare form is, indented or not.
#[test]
fn long_list_entries_are_hidden_in_every_scope() {
    for (content, secret) in [
        (
            "environment:\n  - DB_PASSWORD=hunter2hunter2xyz\n",
            "hunter2hunter2xyz",
        ),
        ("- DB_PASSWORD=hunter2hunter2xyz\r\n", "hunter2hunter2xyz"),
        ("env:\n\t-\tAPI_SECRET=abc123abc123abc\n", "abc123abc123abc"),
    ] {
        for scope in CONFIG_SCOPES {
            hides(content, *scope, &[secret]);
        }
    }
}

/// The list marker only moves where the line starts: a list entry gets the
/// verdict its bare form gets, in every scope, whatever the key and value.
#[test]
fn a_list_entry_is_judged_as_its_bare_form() {
    for key in [
        "DB_PASSWORD",
        "PASSWORD_MIN_LENGTH",
        "LOG_LEVEL",
        "API_TOKEN",
    ] {
        for value in [
            "hunter2",
            "8",
            "debug",
            "hunter2hunter2xyz",
            "${DB_PASSWORD}",
            "/run/secrets/token",
        ] {
            for scope in CONFIG_SCOPES {
                let bare = detect_in(&format!("{key}={value}\n"), *scope).sanitized;
                for marker in ["- ", "  - ", "\t-\t"] {
                    let listed = detect_in(&format!("{marker}{key}={value}\n"), *scope);
                    assert_eq!(
                        listed.sanitized,
                        format!("{marker}{bare}"),
                        "{scope:?} {marker:?}{key}={value}"
                    );
                }
            }
        }
    }
}

/// `NAME: value` mappings (compose `environment:`, Kubernetes `env`, any
/// YAML) hide a value of any length wherever they are read: in YAML, INI and
/// dotenv files, pathless and stored.
#[test]
fn short_mapping_values_are_hidden_in_every_scope() {
    for (content, secret) in [
        ("DB_PASSWORD: hunter2\n", "hunter2"),
        ("  DB_PASSWORD: x\n", "x\n"),
        ("password: hunter2xyz\n", "hunter2xyz"),
        ("secret: abc123\n", "abc123"),
        ("environment:\n  API_TOKEN: abc123\n", "abc123"),
        ("- password: hunter2\r\n", "hunter2"),
    ] {
        for scope in CONFIG_SCOPES {
            let hidden = hides(content, *scope, &[secret.trim_end()]);
            assert!(
                hidden.starts_with(content.split(':').next().unwrap()),
                "{hidden:?}"
            );
        }
    }
}

/// What YAML reads as no string, a reference, a name of another object and a
/// location stay readable.
#[test]
fn mapping_entries_without_a_secret_stay_readable() {
    for content in [
        "automountServiceAccountToken: false\n",
        "password: null\n",
        "token: ~\n",
        "password: ${DB_PASSWORD}\n",
        "secretName: db-secrets\n",
        "tokenFile: /var/run/token\n",
    ] {
        for scope in YAML_SCOPES {
            keeps(content, *scope);
        }
    }
}

/// INI files read `key = value` like properties files.
#[test]
fn ini_settings_hide_short_values_under_credential_keys() {
    for path in ["conf/app.ini", "APP.INI"] {
        for content in [
            "[db]\npassword = hunter2\n",
            "[db]\npassword: hunter2\r\n",
            "[api]\nsecret=abc123\n",
        ] {
            for scope in [
                Scope::File(path),
                Scope::Leaf(path),
                Scope::Stored(Some(path)),
            ] {
                hides(content, scope, &["hunter2", "abc123"]);
            }
        }
    }
}

/// A value that continues on the next line (`\` at a line's end) is one value
/// whatever the rule that read its first line and whatever the line ends: no
/// tail stays readable, and the text of a long source reads the same.
#[test]
fn continued_values_hide_their_tails_in_every_scope_and_line_end() {
    let pathless_and_stored: &[Scope<'static>] = &[
        Scope::Pathless,
        Scope::Stored(None),
        Scope::File("conf/.env"),
        Scope::File("app.properties"),
    ];
    let cases: [(&[Scope<'static>], &str, &str); 4] = [
        (
            pathless_and_stored,
            "DB_PASSWORD=hunter2hunter2xyz",
            "hunter2hunter2xyz",
        ),
        (
            pathless_and_stored,
            "db.password=fakelongword123456789",
            "fakelongword123456789",
        ),
        (
            &[Scope::File("conf/.env"), Scope::Stored(None)],
            "DB_PASSWORD=short",
            "short",
        ),
        (
            &[Scope::File("app.properties")],
            "db.password=fakeword",
            "fakeword",
        ),
    ];
    for (scopes, first, secret) in cases {
        for eol in ["\n", "\r\n", "\r"] {
            let content =
                format!("{first}\\{eol}   part2tailA\\{eol}   part3tailB{eol}next=1{eol}");
            for scope in scopes {
                let hidden = hides(&content, *scope, &[secret, "part2tailA", "part3tailB"]);
                assert!(
                    hidden.ends_with(&format!("next=1{eol}")),
                    "{scope:?} {hidden:?}"
                );
                let long = redact_within(&content, *scope, 1).expect("a secret is hidden");
                assert_eq!(long.sanitized, hidden, "{scope:?} {eol:?} long");
            }
        }
    }
}

/// An even run of backslashes is an escaped backslash, not a continuation.
#[test]
fn an_escaped_backslash_ends_the_value() {
    let content = "DB_PASSWORD=hunter2hunter2xyz\\\\\nnext=visible\n";
    let hidden = hides(content, Scope::Pathless, &["hunter2hunter2xyz"]);
    assert!(hidden.ends_with("\nnext=visible\n"), "{hidden:?}");
}

/// One definition of a line end: `\n`, `\r\n` and a lone `\r`, each kept as it
/// was.
#[test]
fn line_breaks_are_kept_as_written() {
    assert_eq!(kept_line_breaks("a\rb\r\nc\nd"), "\r\r\n\n");
    assert_eq!(line_breaks("\r\r\n\n\n\r").count(), 5);
}

/// Code that reads like a credential entry is no secret: judged by the
/// language's syntax, a statement or member is code, whatever its text spells.
#[test]
fn code_statements_and_members_stay_readable_in_source_files() {
    for (path, content) in [
        ("src/lib.rs", "fn f() {\n    f.evidence.passed = true;\n}\n"),
        (
            "src/lib.rs",
            "struct S {\n    token: &'static str,\n    pub password: String,\n}\n",
        ),
        (
            "src/model.ts",
            "function f() {\n  f.evidence.passed = true;\n}\n",
        ),
        (
            "src/model.ts",
            "const schema = {\n  dispatchToken: z.string().optional(),\n};\n",
        ),
        ("src/model.ts", "interface S {\n  password: string;\n}\n"),
        ("src/model.ts", "const cfg = {\n  password: hunter2,\n};\n"),
        ("tool.mjs", "x.passed = true;\n"),
        ("tool.py", "token: str = None\n"),
    ] {
        for scope in [
            Scope::File(path),
            Scope::Leaf(path),
            Scope::Stored(Some(path)),
        ] {
            keeps(content, scope);
        }
    }
}

/// Text that is not code is config, whatever the path: YAML or INI pasted
/// into a source file parses as a labeled statement, and an object member with
/// a string literal holds a literal secret; an identifier value is code.
#[test]
fn text_that_parses_as_a_label_or_a_literal_is_redacted_in_source_files() {
    for (path, content, secret) in [
        ("src/notes.ts", "password: hunter2xyz\n", "hunter2xyz"),
        ("src/notes.ts", "secret: abc123\n", "abc123"),
        (
            "src/model.ts",
            "const cfg = {\n  password: \"hunter2Sec!x\",\n};\n",
            "hunter2Sec!x",
        ),
    ] {
        for scope in [
            Scope::File(path),
            Scope::Leaf(path),
            Scope::Stored(Some(path)),
        ] {
            hides(content, scope, &[secret]);
        }
    }
}

/// Config text inside a source file's strings and comments is still config:
/// a fixture, a template or a doc comment hides its credentials. A dotenv or
/// properties line applies to stored text only (a source file's own path
/// names neither format).
#[test]
fn config_in_string_literals_and_comments_stays_redacted() {
    for (path, content, secret, stored_only) in [
        (
            "src/fixture.ts",
            "const FIXTURE = `\npassword: hunter2xyz\n`;\n",
            "hunter2xyz",
            false,
        ),
        (
            "src/fixture.rs",
            "const ENV: &str = r#\"\nDB_PASSWORD=hunter2\n\"#;\n",
            "hunter2",
            true,
        ),
        (
            "src/fixture.py",
            "CONFIG = '''\ndb.password=hunter2\n'''\n",
            "hunter2",
            true,
        ),
        (
            "src/fixture.ts",
            "/*\n  secret: abc123\n*/\nconst x = 1;\n",
            "abc123",
            false,
        ),
        (
            "src/fixture.rs",
            "let s = \"\\\n    api_token: hunter2xyz\n\";\n",
            "hunter2xyz",
            false,
        ),
    ] {
        hides(content, Scope::Stored(Some(path)), &[secret]);
        if !stored_only {
            hides(content, Scope::File(path), &[secret]);
        }
    }
}

/// A quoted credential literal in code is `envVarSecrets`' and stays redacted
/// where the syntax judgment applies.
#[test]
fn quoted_credential_literals_in_code_stay_redacted() {
    for (path, content, secret) in [
        (
            "src/db.rs",
            "let password = \"hunter2Sec!x\";\n",
            "hunter2Sec!x",
        ),
        (
            "src/db.ts",
            "const apiKey = 'abcdefghijklmnop1234';\n",
            "abcdefghijklmnop1234",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &[secret]);
        }
    }
}

/// Text that no grammar places stays closed: a source file that does not
/// parse is read as config, so nothing is kept on a guess.
#[test]
fn text_the_grammar_cannot_place_is_read_as_config() {
    hides(
        "const = = {\n  password: hunter2xyz,\n",
        Scope::Stored(Some("src/broken.ts")),
        &["hunter2xyz"],
    );
    hides(
        "fn ((( {\n    password: hunter2xyz\n",
        Scope::Stored(Some("src/broken.rs")),
        &["hunter2xyz"],
    );
}

/// A quoted list entry (`- "NAME=value"`, a compose `environment:` item) is a
/// credential where its bare form is, whichever quote it is written with.
#[test]
fn a_quoted_list_entry_is_judged_as_its_bare_form() {
    for (quoted, bare) in [
        (
            "- \"DB_PASSWORD=Fake0Moo9Zed\"\n",
            "- DB_PASSWORD=Fake0Moo9Zed\n",
        ),
        (
            "- 'DB_PASSWORD=Fake0Moo9Zed'\n",
            "- DB_PASSWORD=Fake0Moo9Zed\n",
        ),
        (
            "  - \"API_TOKEN=Fake0Moo9Zed\"  # from the vault\n",
            "  - API_TOKEN=Fake0Moo9Zed  # from the vault\n",
        ),
    ] {
        for scope in CONFIG_SCOPES {
            let bare_hides = detect_in(bare, *scope).sanitized != bare;
            let quoted_hides = detect_in(quoted, *scope).sanitized != quoted;
            assert_eq!(quoted_hides, bare_hides, "{scope:?}: {quoted:?}");
            if bare_hides {
                hides(quoted, *scope, &["Fake0Moo9Zed"]);
            }
        }
    }
}

/// A quoted YAML scalar is hidden at any length, like an unquoted one.
#[test]
fn short_quoted_mapping_values_are_hidden() {
    for content in [
        "password: 'Fake0'\n",
        "password: \"Fake0\"\n",
        "  DB_PASSWORD: 'Fake0'  # staging\n",
        "- secret: \"Fake0\"\r\n",
    ] {
        for scope in CONFIG_SCOPES {
            hides(content, *scope, &["Fake0"]);
        }
    }
    for kept in [
        "password: '${DB_PASSWORD}'\n",
        "password: \"<db-pass-1>\"\n",
    ] {
        for scope in YAML_SCOPES {
            keeps(kept, *scope);
        }
    }
}

/// A match inside a string literal or comment is never code, even where the
/// literal closes on the match's own line.
#[test]
fn a_literal_closing_on_the_matched_line_is_still_a_literal() {
    for (path, content) in [
        ("src/a.ts", "const y = `\npassword: Fake0Moo9Zed`;\n"),
        ("src/a.ts", "const y = `\r\npassword: Fake0Moo9Zed`;\r\n"),
        ("src/a.rs", "let y = r#\"\npassword: Fake0Moo9Zed\"#;\n"),
        ("src/a.ts", "/*\npassword: Fake0Moo9Zed */\n"),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &["Fake0Moo9Zed"]);
        }
    }
}

/// A plain YAML scalar of several words is one value: the rest of the line,
/// less a trailing comment.
#[test]
fn a_multi_word_scalar_is_hidden_whole() {
    for (content, secrets) in [
        (
            "password: correct Fake0Moo9Zed phrase\n",
            &["correct", "Fake0Moo9Zed", "phrase"][..],
        ),
        (
            "  api_token: Fake0Moo9Zed two words # note\n",
            &["Fake0Moo9Zed", "two words"],
        ),
        (
            "password: Fake0Moo9Zed \\\n  second Fake1Moo9Zed\n",
            &["Fake0Moo9Zed", "Fake1Moo9Zed"],
        ),
    ] {
        for scope in YAML_SCOPES {
            let hidden = hides(content, *scope, secrets);
            assert!(hidden.starts_with(content.split(':').next().unwrap()));
        }
    }
    hides(
        "secret: Fake0Moo9Zed words # note\n",
        Scope::File("a.yml"),
        &["Fake0Moo9Zed words"],
    )
    .contains("# note")
    .then_some(())
    .expect("the trailing comment stays");
}

/// A credential literal in code is redacted however it is written: a backtick
/// or raw string, a typed declaration, a fallback after `||`.
#[test]
fn credential_literals_of_every_form_in_code_are_redacted() {
    for (path, content) in [
        ("src/db.ts", "const password = `Fake0Moo9Zed`;\n"),
        (
            "src/db.ts",
            "const o = {\n  password: `Fake0Moo9Zed`,\n};\n",
        ),
        ("src/db.go", "package db\n\nvar password = `Fake0Moo9Zed`\n"),
        ("src/db.rs", "const PASSWORD: &str = \"Fake0Moo9Zed\";\n"),
        (
            "src/db.rs",
            "let db_password: String = \"Fake0Moo9Zed\".to_owned();\n",
        ),
        (
            "src/db.ts",
            "const pw = process.env.DB_PASSWORD || \"Fake0Moo9Zed\";\n",
        ),
        (
            "src/db.ts",
            "const pw = process.env.DB_PASSWORD ?? 'Fake0Moo9Zed';\n",
        ),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &["Fake0Moo9Zed"]);
        }
    }
}

/// A commented-out credential is still a credential: a line comment of any
/// language, a JSDoc line, a dotenv or YAML comment.
#[test]
fn commented_out_credentials_are_redacted() {
    for (path, content) in [
        ("src/db.ts", "// DB_PASSWORD=Fake0Moo9Zed\nconst x = 1;\n"),
        (
            "src/db.ts",
            "/**\n * password: Fake0Moo9Zed\n */\nconst x = 1;\n",
        ),
        ("src/db.py", "# password: Fake0Moo9Zed\nx = 1\n"),
        ("src/db.rs", "// api_token: Fake0Moo9Zed\nfn f() {}\n"),
        ("conf/.env", "# DB_PASSWORD=Fake0Moo9Zed\n"),
        ("app.properties", "! db.password=Fake0Moo9Zed\n"),
        ("app.properties", "# db.password=Fake0Moo9Zed\n"),
        ("docker-compose.yml", "# password: Fake0Moo9Zed\n"),
    ] {
        for scope in [Scope::File(path), Scope::Stored(Some(path))] {
            hides(content, scope, &["Fake0Moo9Zed"]);
        }
    }
    // A comment that only talks about a credential stays readable.
    keeps(
        "// the password: required\nconst x = 1;\n",
        Scope::File("src/a.ts"),
    );
}

/// A line of several pairs names each key by its own text: an earlier pair's
/// colon is no type annotation of the key that follows.
#[test]
fn a_key_after_other_pairs_on_one_line_is_its_own_name() {
    keeps(
        "{\"keys_url\":\"https://api.example.test/keys\",\"secretPath\":\"src/secrets/credentials/private/source.pem\"}\n",
        Scope::Pathless,
    );
}
