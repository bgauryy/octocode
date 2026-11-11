//! Credential assignments as source lays them out: comments and line breaks
//! between the name, its operator and its value, a numbered name, and a
//! value that ends where its comment does.

use super::*;

const CHOSEN: &str = "hunter2-Sup3rS3cret";
const OPAQUE: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ8vT3kLm2";
const HEX: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

/// A comment between the name, the operator and the value is no part of
/// any of them: the assignment is read through it, in every comment form a
/// source file writes there, and the comments stay.
#[test]
fn an_assignment_is_read_through_the_comments_inside_it() {
    for (content, path, secret) in [
        (
            format!("  /* block */ dbPassword /* c1 */ : /* c2 */ \"{CHOSEN}\" /* c3 */,\n"),
            "src/config.ts",
            CHOSEN,
        ),
        (
            format!("const AWS_SECRET_ACCESS_KEY: &str = /* inline */ \"{OPAQUE}\";\n"),
            "src/config.rs",
            OPAQUE,
        ),
        (
            format!("const apiKey /* the * starred ** one */ = /**/ \"{OPAQUE}\";\n"),
            "src/config.ts",
            OPAQUE,
        ),
        (
            format!("const apiKey = // the key\n  \"{OPAQUE}\";\n"),
            "src/config.ts",
            OPAQUE,
        ),
        (
            format!("api_key = \\\n    # the key\n    \"{OPAQUE}\"\n"),
            "settings.py",
            OPAQUE,
        ),
    ] {
        for path in [Some(path), None] {
            assert_redacts_only(&content, path, secret);
        }
    }
}

/// The pieces of one literal are one value through a comment between them:
/// no piece stays, and the statement around the literal does.
#[test]
fn a_literal_in_pieces_is_one_value_through_the_comments_between_them() {
    let (head, tail) = OPAQUE.split_at(20);
    for seam in ["/* mid */ + ", "+ // joined below\n  ", "\\\n    "] {
        let content = format!("const authToken = \"{head}\" {seam}\"{tail}\";\n");
        for path in [Some("src/config.ts"), None] {
            let result = detect(&content, path);
            assert!(!result.sanitized.contains(head), "{path:?}: {content:?}");
            assert!(!result.sanitized.contains(tail), "{path:?}: {content:?}");
            assert!(
                result
                    .sanitized
                    .starts_with("const authToken = \"[REDACTED-")
                    && result.sanitized.ends_with("\";\n"),
                "{path:?}: {:?}",
                result.sanitized
            );
            assert_eq!(
                result.sanitized.lines().count(),
                content.lines().count(),
                "{path:?}: {:?}",
                result.sanitized
            );
        }
    }
}

/// A value on the lines after its name is that name's value: after the
/// operator alone, inside the parentheses that wrap it, or indented under a
/// YAML key.
#[test]
fn an_assignment_is_read_to_a_value_on_the_next_lines() {
    for (content, path, secret) in [
        (
            format!("const apiKey =\n  // the key\n  \"{OPAQUE}\";\n"),
            "src/config.ts",
            OPAQUE,
        ),
        (
            format!("AWS_SECRET_ACCESS_KEY = (\n    # wrapped\n    \"{OPAQUE}\"\n)\n"),
            "settings.py",
            OPAQUE,
        ),
        (
            format!(
                "    let api_key = String::from(\n        // key\n        \"{OPAQUE}\",\n    );\n"
            ),
            "src/config.rs",
            OPAQUE,
        ),
        (format!("api_key:\n  {HEX}\n"), "deploy/values.yaml", HEX),
        (
            format!("db:\n  password:   # rotated\n    {CHOSEN}\n  port: 5432\n"),
            "deploy/values.yaml",
            CHOSEN,
        ),
    ] {
        for path in [Some(path), None] {
            assert_redacts_only(&content, path, secret);
        }
    }
}

/// A key's lines that hold no scalar of its own stay: a nested mapping, a
/// list, a block scalar's indicator, the next key.
#[test]
fn a_yaml_key_without_a_scalar_on_the_next_line_reads_nothing() {
    for content in [
        "secrets:\n  - registry-pull-secret\n  - another-one\n",
        "credentials:\n  username: deploy\n",
        "api_key:\nother_setting: plain-value-here\n",
        "token:\n\nnext: value\n",
    ] {
        for path in [Some("deploy/values.yaml"), None] {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
        }
    }
}

/// A digit after the credential word numbers the name (`apiKey2`,
/// `API_KEY_2`, `authToken9`): it names a credential as the bare name does.
#[test]
fn a_numbered_credential_name_holds_a_credential() {
    for name in ["API_KEY2", "DB_PASSWORD_1", "SERVICE_TOKEN9"] {
        let content = format!("{name}={OPAQUE}\n");
        assert_redacts_only(&content, None, OPAQUE);
    }
    for (key, secret) in [
        ("apiKey2", OPAQUE),
        ("apiKey", OPAQUE),
        ("authToken9", "sk.live.Xk9Qz7Lm2Vp4"),
        ("authToken", "sk.live.Xk9Qz7Lm2Vp4"),
        ("API_KEY2", OPAQUE),
        ("dbPassword1", CHOSEN),
        ("secret2", CHOSEN),
    ] {
        for (content, path) in [
            (
                format!("const {key} = \"{secret}\";\n"),
                Some("src/config.ts"),
            ),
            (format!("const {key} = \"{secret}\";\n"), None),
            (
                format!("{{\"{key}\": \"{secret}\"}}\n"),
                Some("fixtures/row.json"),
            ),
            (format!("{key}: {secret}\n"), Some("deploy/values.yaml")),
        ] {
            assert_redacts_only(&content, path, secret);
        }
    }
}

/// A value in a block comment ends where the comment does: the closing
/// delimiter is the comment's, not the value's.
#[test]
fn a_commented_value_keeps_the_delimiter_that_closes_its_comment() {
    let slack = "xoxb-2048576310-4096152620-AbCdEfGhIjKlMnOpQrStUvWx";
    for (content, path, secret) in [
        (
            format!("/* slack_token: {slack} */\n"),
            "src/config.rs",
            slack,
        ),
        (
            format!("/* db_password: {CHOSEN} */\n"),
            "src/config.rs",
            CHOSEN,
        ),
        (
            format!("/* db_password: {CHOSEN} */\n"),
            "src/config.ts",
            CHOSEN,
        ),
        (
            format!("<!-- db_password: {CHOSEN} -->\n"),
            "docs/setup.md",
            CHOSEN,
        ),
        (
            format!("/** db_password: {CHOSEN} **/\n"),
            "src/config.ts",
            CHOSEN,
        ),
    ] {
        for path in [Some(path), None] {
            assert_redacts_only(&content, path, secret);
        }
    }
}

/// A token written as the pieces of one literal is the token the program
/// joins: each piece's part of it is hidden where it is written, under any
/// name, and the operator, the quotes and a comment between the pieces stay.
#[test]
fn a_token_written_in_literal_pieces_is_hidden_piece_by_piece() {
    let token = format!("ghp_{}", "x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI");
    let (tag, body) = token.split_at(4);
    let (head, tail) = token.split_at(20);
    for (content, path) in [
        (
            format!("const split = \"{tag}\" + // joined below\n  \"{body}\";\n"),
            "src/config.ts",
        ),
        (
            format!("const split2 = \"{head}\" /* mid */ + \"{tail}\";\n"),
            "src/config.ts",
        ),
        (
            format!("value = \"{tag}\" \\\n    \"{body}\"\n"),
            "settings.py",
        ),
        (
            format!("char *value = \"{head}\"\n    \"{tail}\";\n"),
            "src/config.c",
        ),
        (
            format!("let s = 'before ' + '{head}' + \"{tail}\" + ' after';\n"),
            "src/config.ts",
        ),
    ] {
        for path in [Some(path), None] {
            let result = detect(&content, path);
            for part in [body, head, tail] {
                assert!(
                    !result.sanitized.contains(part),
                    "{path:?} leaked {part}: {:?}",
                    result.sanitized
                );
            }
            let seams = crate::security::placeholder_regex()
                .split(&result.sanitized)
                .collect::<String>();
            let written = content.replace(tag, "").replace(body, "");
            let written = written
                .replace(head.trim_start_matches(tag), "")
                .replace(tail, "");
            assert_eq!(seams, written, "{path:?}: {:?}", result.sanitized);
        }
    }
}

/// A list whose items, from one that opens with a pattern's own literal,
/// are joined exactly one secret of that pattern is that secret written in
/// pieces, whatever joins it (`concat!`, `.join("")`): each of those items
/// goes, and the list around them stays, an item before or after them too.
/// A literal a template writes in a hole of its own is a piece as well.
#[test]
fn a_list_that_spells_exactly_one_token_is_that_token_in_pieces() {
    let body = "x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
    let (head, tail) = body.split_at(20);
    for (content, pieces) in [
        (
            format!("const PARTS: &str = concat!(\"ghp_\", \"{body}\");\n"),
            vec!["ghp_", body],
        ),
        (
            format!("const d = [\"ghp_\", \"{head}\", \"{tail}\"].join(\"\");\n"),
            vec!["ghp_", head, tail],
        ),
        (
            format!("const d = [\n  'ghp_', // the tag\n  '{body}',\n];\n"),
            vec!["ghp_", body],
        ),
        (
            format!("const d = [\"Bearer \", \"ghp_\", \"{body}\", \"-suffix\"];\n"),
            vec!["ghp_", body],
        ),
        (
            format!("const e = `ghp_${{\"{body}\"}}`;\n"),
            vec!["ghp_", body],
        ),
        (
            format!("const e = `ghp_${{ '{body}' }} `;\n"),
            vec!["ghp_", body],
        ),
    ] {
        let mut expected = content.clone();
        for piece in pieces {
            expected = expected.replacen(piece, "[REDACTED-GITHUBTOKENS]", 1);
        }
        for path in [Some("src/config.ts"), None] {
            assert_eq!(detect(&content, path).sanitized, expected, "{path:?}");
        }
    }
}

/// Literals that only sit side by side stay: a list's items are values of
/// their own unless together they are exactly one token, and pieces that
/// join into no secret are text.
#[test]
fn literals_that_spell_no_secret_together_stay() {
    for content in [
        "const parts = [\"ghp_\", \"too short\"];\n",
        "const parts = [\"ghp_\", \"x7Kq2Lm9Zp4Vw8Rt1Yb6\", \"and more words\"];\n",
        "const tags = [\"ghp_\", \"gho_\", \"ghu_\", \"ghs_\", \"ghr_\"];\n",
        "const names = [\"token\", \"password\", \"secret\", \"apiKey\"];\n",
        "const e = `ghp_${name}` + `${\"suffix\"}`;\n",
        "const label = \"hello \" + \"world\" + ' and more';\n",
        "printf(\"%s\" \"%d\", name, count);\n",
    ] {
        for path in [Some("src/config.ts"), None] {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
        }
    }
}

/// A value prose quotes as code ends where the code span does: the words of
/// the sentence after the closing backtick are no part of it, in a source
/// file's comment and in a document alike.
#[test]
fn a_value_quoted_in_prose_ends_with_its_code_span() {
    for (content, path, secret) in [
        (
            "        // A connection string (`Server=db;User Id=sa;Password=x`) sits under\n",
            "src/patterns.rs",
            "x",
        ),
        (
            "/// literal's close (`r\"Server=db;Password=x7Kq2Lm9\\\"`, `` `…Pwd=x` ``), so a rule\n",
            "src/detector.rs",
            "x7Kq2Lm9\\\"",
        ),
        (
            "Give it a connection string (`Server=db;Uid=sa;Pwd=x7Kq2Lm9Zp`) and run it again.\n",
            "docs/setup.md",
            "x7Kq2Lm9Zp",
        ),
        (
            "/// A dotenv line (`API_TOKEN=x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3H`) wherever it sits in the text.\n",
            "src/detector.rs",
            "x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3H",
        ),
        (
            "Set it (`DB_PASSWORD=x7Kq2Lm9Zp4Vw8Rt`) before the first run, then restart.\n",
            "docs/setup.md",
            "x7Kq2Lm9Zp4Vw8Rt",
        ),
    ] {
        let result = detect(content, Some(path));
        let restored = crate::security::placeholder_regex().replace_all(&result.sanitized, secret);
        assert_eq!(restored, content, "{path}: {:?}", result.sanitized);
    }
}
