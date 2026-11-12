//! Final round (2026-11-12): a literal is judged as the program holds it
//! over its lines, a config entry a literal writes loses only its value, and
//! a request is read as written where a read shows it. Each rule hides only
//! the secret, in a file and in the pathless net alike, and a request is
//! refused exactly where a read hides.

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

/// That `content` loses `secrets` at every path given and in the pathless
/// net, whatever placeholders stand for them, and that a request holding it
/// is refused.
fn hidden(content: &str, paths: &[&str], secrets: &[&str]) {
    for path in paths.iter().copied().map(Some).chain([None]) {
        let shown = detect(content, path).sanitized;
        for secret in secrets {
            assert!(!shown.contains(secret), "{path:?} shows {secret}: {shown}");
        }
        assert_eq!(
            shown.lines().count(),
            content.lines().count(),
            "{path:?}: {shown}"
        );
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

/// Item 1 (round 15 retest): a credential-named literal whose line ends
/// inside it, at a real line break or at a backslash that continues it, is
/// one value with the lines it goes on over. The same value on another line
/// of the file was always hidden; now every line of it is.
#[test]
fn a_literal_over_lines_is_one_value() {
    let paths = ["app/settings.py", "src/a.ts", "notes.txt"];
    let (head, tail) = KEY.split_at(20);
    // The retest's file: the parameter's default and the open literal.
    let file = format!(
        "import os\n\ndef f(api_key=\"{KEY}\"):\n    return api_key\ntoken = \"{KEY}\n\"\n"
    );
    hidden(&file, &["app/settings.py", "notes.txt"], &[KEY]);
    for content in [
        // A real line break, the close on the next line.
        format!("token = \"{KEY}\n\"\n"),
        format!("token = '{KEY}\n'\n"),
        format!("token = `{KEY}\n`;\n"),
        // A backslash continues the literal; the next line goes on with it.
        format!("token = \"{KEY}\\\nmore\"\n"),
        format!("token = \"{KEY}\\\n\"\n"),
        // A key written over two lines, either way.
        format!("token = \"{head}\n{tail}\"\n"),
        format!("token = \"{head}\\\n{tail}\"\n"),
    ] {
        hidden(&content, &paths, &[KEY, head, tail]);
    }
    // Each continued line that is a value alone loses only its text: the
    // backslash, the break and the next line's indent stay.
    hidden_alone(
        &format!("    let private_key = \"{KEY}\\\n        {KEY}\";\n"),
        &["src/lay.rs", "notes.txt"],
        &[KEY, KEY],
    );
    // A line of words after an open literal is no part of its value, and a
    // literal of words over lines is prose.
    for content in [
        "message = \"the token is\nmissing here\"\n",
        "help_token = \"see the docs \\\n  for the token\"\n",
    ] {
        shown_and_run(content, &paths);
    }
}

/// Item 2: a literal that writes a config entry under a credential's name
/// (an `.npmrc` line) loses the entry's value alone, under any name: the
/// registry, the `_authToken=` key and the literal's `\n` stay.
#[test]
fn a_literal_config_entry_loses_only_its_value() {
    let paths = ["app/settings.py", "src/fields.rs", "notes.txt"];
    for content in [
        format!("x_token = \"//registry.npmjs.org/:_authToken={KEY}\\n\"\n"),
        format!("    let npm_token = \"//registry.npmjs.org/:_authToken={KEY}\\n\";\n"),
        format!("x_token = \"//registry.npmjs.org/:_authToken={KEY}\"\n"),
        format!("npmrc = \"//registry.npmjs.org/:_authToken={KEY}\\n\"\n"),
    ] {
        hidden_alone(&content, &paths, &[KEY]);
    }
    // Two entries, each value its own.
    let other = "Hd5Jf0GsAeUoIx7Kq2Lm9Zp4Vw8RbN3c6Yt1";
    hidden_alone(
        &format!(
            "x_token = \"//npm.corp.example/:_authToken={KEY}\\n//localhost:4873/:_authToken={other}\\n\"\n"
        ),
        &paths,
        &[KEY, other],
    );
    // A reference in the entry is no value.
    shown_and_run(
        "x_token = \"//registry.npmjs.org/:_authToken=${NPM_TOKEN}\\n\"\n",
        &paths,
    );
}

/// Item 3: a request is read as written, as a read reads the same line: an
/// escaped quote in a line of code is no regex escape, so the line a read
/// shows runs, and the one a read hides is refused. A regex spelling still
/// reads as the literal it matches.
#[test]
fn a_request_reads_an_escaped_quote_as_written() {
    let call = "    let cs_credential = format!(\"  var cred = new NetworkCredential(\\\"svc\\\", \\\"{PASSWORD}\\\");\\n\");\n";
    shown_and_run(call, &["src/fields.rs"]);
    assert_eq!(literal_form("say \\\"hi\\\" now"), None);
    assert_eq!(
        literal_form("say \\\"hi\\\"\\.now").as_deref(),
        Some("say \\\"hi\\\".now")
    );
    let quoted = format!("    let cs = \"  password = \\\"{KEY}\\\";\";\n");
    hidden_alone(&quoted, &["src/fields.rs"], &[KEY]);
    let token = "ghp_x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
    let spelled = token.replace('_', "\\_");
    assert!(holds_credential_as_request(&spelled), "ran {spelled:?}");
}
