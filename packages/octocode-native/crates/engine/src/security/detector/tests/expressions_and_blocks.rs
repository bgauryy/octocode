//! Round 13 and 14 retests: a YAML block scalar's body is its key's value, an
//! expression is no value (only its literals are), a template literal's
//! holes are code, a documented placeholder and an identifier constant are
//! no secret, and a value never runs past what code writes for it. Each rule
//! hides only the secret, in a file and in the pathless net alike, and a
//! request is refused exactly where a read hides.

use super::*;

const KEY: &str = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";

/// That `content` loses each of `secrets` and nothing else at every path
/// given and in the pathless net, and that a request holding it is refused.
fn hidden_alone(content: &str, paths: &[&str], secrets: &[&str]) {
    for path in paths.iter().copied().map(Some).chain([None]) {
        assert_redacts_each(content, path, secrets);
    }
    assert!(holds_credential_as_request(content), "ran {content:?}");
}

/// That `content` stays as written at every path given and in the pathless
/// net, and runs as a request.
fn shown_and_run(content: &str, paths: &[&str]) {
    for path in paths.iter().copied().map(Some).chain([None]) {
        assert_eq!(detect(content, path).sanitized, content, "{path:?}");
    }
    assert!(!holds_credential_as_request(content), "refused {content:?}");
}

/// D1: the lines of a block scalar under a credential's key are its value,
/// judged as the one-line `key: value` is: each line's text goes, the
/// header and the indentation stay, in a YAML file, a text file, a fenced
/// block and the pathless net.
#[test]
fn a_block_scalar_body_is_its_keys_value() {
    let paths = ["config/db.yaml", "notes.txt", "README.md"];
    for header in ["|", "|-", ">", ">-"] {
        for key in ["password", "api_key", "secret", "db_password"] {
            let content = format!("db:\n  {key}: {header}\n    Fz8kQ2mL9xW4\n  host: localhost\n");
            hidden_alone(&content, &paths, &["Fz8kQ2mL9xW4"]);
            let shown = detect(&content, None).sanitized;
            assert!(
                shown.starts_with(&format!("db:\n  {key}: {header}\n    [REDACTED-")),
                "{shown:?}"
            );
            assert!(shown.ends_with("]\n  host: localhost\n"), "{shown:?}");
        }
    }
    // A body of several lines loses each line's text.
    let content = "secret: |-\n  Fz8kQ2mL9xW4\n  Qm7vR2xL8pZ3\nnext: 1\n";
    hidden_alone(content, &paths, &["Fz8kQ2mL9xW4", "Qm7vR2xL8pZ3"]);
    // Fenced in prose, the block is the same block.
    hidden_alone(
        "Set it:\n\n```yaml\ndb:\n  password: |\n    Fz8kQ2mL9xW4\n```\n",
        &["README.md"],
        &["Fz8kQ2mL9xW4"],
    );
    for content in [
        "description: |\n  Fz8kQ2mL9xW4 is no key's\n",
        "password: |\n  ${DB_PASSWORD}\n",
        "token: |\n  true\n",
        "signing_key: |\n  -----BEGIN PGP PUBLIC KEY BLOCK-----\n  mQINBGRkZm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9v\n  -----END PGP PUBLIC KEY BLOCK-----\n",
    ] {
        shown_and_run(content, &["config/db.yaml", "notes.txt"]);
    }
}

/// D5: an expression is no secret value, as code and in a request: a call,
/// a member access asserted non-null, `??` or `||` with no literal, an
/// index. A literal inside one is judged as the name's value is.
#[test]
fn an_expression_is_no_value_and_only_its_literals_are_judged() {
    let paths = ["notes.txt", "config/app.yaml", "src/session-store.mts"];
    for content in [
        "  token: randomBytes(16).toString('hex'),\n",
        "  apiKey: process.env.API_KEY!,\n",
        "token: getToken()\n",
        "  password: hash(pw),\n",
        "  token: opts.token ?? defaultToken,\n",
        "  secret: tokenStore[key],\n",
        "  apiKey: null,\n",
        "  token: this.token,\n",
        "  password: undefined,\n",
    ] {
        shown_and_run(content, &paths);
    }
    hidden_alone(
        &format!("  token: opts.token ?? '{KEY}',\n"),
        &["notes.txt", "src/session-store.mts"],
        &[KEY],
    );
    // A value no code word spells, or a chosen password's mark, is a value.
    for (content, secret) in [
        ("password: Zq7Xk2Pm9Vt4(x)\n", "Zq7Xk2Pm9Vt4(x)"),
        ("password: Welcome!\n", "Welcome!"),
    ] {
        hidden_alone(content, &["notes.txt"], &[secret]);
    }
}

/// D4: a template literal's holes are code, its nested strings included:
/// outside them `Basic` is a scheme word, no credential. A key written in
/// a hole's own string is still a literal of the expression.
#[test]
fn a_template_literal_is_judged_outside_its_holes() {
    let paths = ["src/transport.ts", "notes.txt"];
    shown_and_run(
        "h.Authorization = `Basic ${Buffer.from(`${u}:${p}`).toString('base64')}`;\n",
        &paths,
    );
    hidden_alone(
        &format!(
            "h.Authorization = `Basic ${{Buffer.from(`${{u}}:{KEY}`).toString('base64')}}`;\n"
        ),
        &paths,
        &[KEY],
    );
    hidden_alone(
        &format!("h.Authorization = `Bearer {KEY}`;\n"),
        &paths,
        &[KEY],
    );
}

/// H1: a backtick a comment writes opens no literal: no value runs from it
/// into the code lines below, and a comparison with a mark (`~`) holds no
/// secret. The detector's own source reads as written.
#[test]
fn a_value_never_runs_past_its_line_into_code() {
    let source = "/// Whether `secret` after `key` is a sentence: after `name:` several words\n/// are prose (`password: must be at least 8 characters`); after `NAME=` they\n/// are a value.\npub(super) fn is_sentence(key: &impl KeyFacts, secret: &str) -> bool {\n    key.ends_with_colon() && secret.contains(char::is_whitespace)\n}\n\n/// Characters of a comment's first word after `name:` that make it a secret\nconst COMMENT_OPAQUE_WORD_CHARS: usize = 8;\n\nfn is_non_string_scalar(secret: &str) -> bool {\n    secret == \"~\"\n        || [\"true\", \"false\", \"null\"].iter().any(|k| secret.eq_ignore_ascii_case(k))\n}\n";
    for path in [Some("src/credential_values.rs"), Some("notes.txt"), None] {
        assert_eq!(detect(source, path).sanitized, source, "{path:?}");
    }
    // Code an operator-stated word took whole: a JSON member's code value,
    // a dotenv-like line inside a Rust string, a regex in a raw string, a
    // browser's flag list.
    for content in [
        "json!({\"key\":field.path,\"value\":shown(field,get(&config.resolved,field.path)),\"envKey\":env_key})\n",
        "let npmrc = \"//npm.corp.example/:_authToken=abc\\n//localhost:4873/:_authToken=local\\n\";\n",
        "regex: r###\"\"client_secret\":\\s*\"(?P<secret>[a-zA-Z0-9-_]{24})\"\"###,\n",
        "args = [\n  \"--password-store=basic\",\n  \"--use-mock-keychain\",\n]\n",
        "auth(\"https://npm.example.com/\", npmrc)\n",
        "const MACRO_TOKEN_TREES: [&str; 3] = [\"token_tree\", \"token_tree_pattern\", \"token_binding_pattern\"];\n",
        "pub(crate) const GITHUB_AUTH_RECOVERY_HINT: &str = \"Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.\";\n",
    ] {
        for path in [Some("src/a.rs"), Some("notes.txt"), None] {
            assert_eq!(detect(content, path).sanitized, content, "{path:?}");
        }
    }
    // Prose a string holds loses its secret only, not the code after it.
    let shown = detect(
        "x(\"the password is Tr0ub4dor&3xK9!zQ\".to_owned());\n",
        None,
    )
    .sanitized;
    assert!(shown.ends_with("\".to_owned());\n"), "{shown:?}");
    assert!(!shown.contains("Tr0ub4dor"), "{shown:?}");
}

/// D9 and the round-13 GitHub check: a documented placeholder (`your_…`,
/// `YOUR_…`, `<…>`) and a constant that is an identifier (a dunder name) are
/// no secret; a real value pasted into a placeholder still is.
#[test]
fn a_documented_placeholder_and_an_identifier_constant_show() {
    for content in [
        "export GITHUB_TOKEN=\"ghp_your_ghe_token\"                       # or use device login:\n",
        "export GITHUB_TOKEN=ghp_your_ghe_token\n",
        "API_KEY=YOUR_API_KEY\n",
        "token: \"<your-token>\"\n",
        "export const ssrImportKey = `__vite_ssr_import__`\n",
        // Round 14, D: a dunder name's words may be camelCase.
        "export const ssrExportAllKey = `__vite_ssr_exportAll__`\n",
        "export const ssrDynamicImportKey = `__vite_ssr_dynamic_import__`\n",
        "export const ssrExportNameKey = `__vite_ssr_exportName__`\n",
    ] {
        shown_and_run(content, &["docs/AUTHENTICATION.md", "src/constants.ts"]);
    }
    hidden_alone(
        "export GITHUB_TOKEN=\"ghp_your_Hunter2_token\"\n",
        &["docs/AUTHENTICATION.md"],
        &["ghp_your_Hunter2_token"],
    );
    // A run of short camelCase parts, or digits in a part, is no word.
    for value in [
        "__vite_zqXkPmVtLwRbNcHyJdFgTsQeUa__",
        "__vite_ssr_export9Qz7Kp__",
    ] {
        hidden_alone(
            &format!("export const ssrApiKey = `{value}`\n"),
            &["src/constants.ts"],
            &[value],
        );
    }
}

/// Round 14, F: inside a string literal a control escape (`\n`, `\t`) ends a
/// value as a real line break does, and a format argument (`{name}`) is no
/// value: the rules for placeholders, prose and templates judge each escaped
/// line, so a source's format templates and sentences show whole, keys and
/// closing `\n"` included, as the same lines of a text file do.
#[test]
fn a_literals_escaped_line_ends_its_value() {
    for content in [
        "        \"the password is required\\n\",\n",
        "        \"the password is hunter2\\n\",\n",
        "        \"the password is <your-password>\\n\",\n",
        "        \"the password is ${DB_PASSWORD}\\n\",\n",
        "        (format!(\"the password is {symbols}\\n\"), symbols),\n",
        "    let long = format!(\"{TOKEN}{OTHER}\");\n",
        "            format!(\"const apiKey = \\\"{long}{escape}Host: example.com\\\";\\n\"),\n",
        "    let npm_token = format!(\"//registry.npmjs.org/:_authToken={TOKEN}\\n\");\n",
        "    let npm_password = format!(\"//npm.pkg.github.com/:_password={PADDED}\\n\");\n",
        "    let continued = format!(\"    let private_key = \\\"\\\\\\n        {TOKEN}\\\";\\n\");\n",
    ] {
        shown_and_run(content, &["src/fields.rs"]);
    }
    // A read shows a call a literal holds whole. (A request reads its `\"` as
    // a regex's escaped quote, a different text: [`literal_form`].)
    let call = "    let cs_credential = format!(\"  var cred = new NetworkCredential(\\\"svc\\\", \\\"{PASSWORD}\\\");\\n\");\n";
    for path in [Some("src/fields.rs"), None] {
        assert_eq!(detect(call, path).sanitized, call, "{path:?}");
    }
    for (content, secret) in [
        (format!("        \"the password is {KEY}\\n\",\n"), KEY),
        (
            format!("    let api_key = format!(\"{{prefix}}{KEY}\\n\");\n"),
            KEY,
        ),
    ] {
        hidden_alone(&content, &["src/fields.rs"], &[secret]);
    }
    // A written value after the key the literal spells goes; in a source,
    // whose syntax reads the escape, its escaped line end stays.
    let written = format!("    let npm_token = \"//registry.npmjs.org/:_authToken={KEY}\\n\";\n");
    let shown = detect(&written, Some("src/fields.rs")).sanitized;
    assert!(shown.ends_with("]\\n\";\n"), "{shown}");
    for path in [Some("src/fields.rs"), None] {
        assert!(!detect(&written, path).sanitized.contains(KEY), "{path:?}");
    }
    assert!(holds_credential_as_request(&written), "ran {written:?}");
}

/// Round 14, F: a heredoc a literal holds (`"token = <<~EOS\n  …\nEOS\n"`)
/// ends at its escaped tag line, and a statement a literal holds
/// (`"let secret = format!(\"…\");\n"`) at its own `;`: neither runs over the
/// source lines after it, which keep their names and values as written.
#[test]
fn a_block_a_literal_holds_ends_inside_it() {
    let source = format!(
        "    let ruby_heredoc = format!(\"token = <<~EOS\\n  {{TOKEN}}\\nEOS\\n\");\n    let format_macro = \"    let secret = format!(\\\"{{}}{{}}\\\", \\\"lz24OfsUehXx\\\", \\\"NiytEBdUaQnw\\\");\\n\";\n    let fixtures = [(format_macro, \"lay.rs\", &[\"lz24OfsUehXx\", \"NiytEBdUaQnw\"])];\n    let inline_env = \"PGPASSWORD={KEY} psql -h db app\\n\";\n"
    );
    let shown = detect(&source, Some("src/layouts.rs")).sanitized;
    assert!(shown.contains("\"PGPASSWORD=[REDACTED-"), "{shown}");
    assert!(shown.contains(" psql -h db app\\n\";"), "{shown}");
    assert!(
        shown.contains("&[\"lz24OfsUehXx\", \"NiytEBdUaQnw\"])];"),
        "{shown}"
    );
    assert_redacts_each(&source, Some("src/layouts.rs"), &[KEY]);
}

/// Round 14, H: a value that reads as a sentence about the value (several
/// plain words, a function word among them, no digit or mark mixed into a
/// word) is prose, in a scalar and in a block scalar's body alike; a phrase of
/// open words alone may be a chosen password and goes, as a value with a
/// mixed word does.
#[test]
fn a_sentence_under_a_credentials_name_is_prose() {
    for content in [
        "password: must be at least 8 characters\n",
        "password: |\n  must be at least 8 characters\n",
        "password: >-\n  must be set by the operator\n  before the first deploy\n",
        "  token: is rotated by the vault agent\n",
    ] {
        shown_and_run(content, &["notes.txt", "docs/setup.md", "conf/app.yaml"]);
    }
    for (content, secret) in [
        (
            "password: |\n  correct horse battery staple\n",
            "correct horse battery staple",
        ),
        ("password: Hunter2 is the one\n", "Hunter2 is the one"),
        ("password: correct horse 9\n", "correct horse 9"),
    ] {
        for path in [Some("notes.txt"), Some("conf/app.yaml"), None] {
            assert!(
                !detect(content, path).sanitized.contains(secret),
                "{path:?}: {content:?}"
            );
        }
        assert!(holds_credential_as_request(content), "ran {content:?}");
    }
}
