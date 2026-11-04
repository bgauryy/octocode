//! How a block's edges and the lines inside it decide what stays hidden:
//! one line holding both edges, a line that is not key text inside a body, and
//! the headers a mention must not take with it.
use super::*;

/// `count` fake base64 lines a wrapped key body has.
fn body(seed: char, count: usize) -> Vec<String> {
    (0..count)
        .map(|at| format!("{seed}{at}{}", "Qx7Zk2Vb9Ls4".repeat(5)))
        .collect()
}

fn pem(lines: &[String]) -> String {
    let mut out = vec!["-----BEGIN RSA PRIVATE KEY-----".to_owned()];
    out.extend_from_slice(lines);
    out.push("-----END RSA PRIVATE KEY-----".to_owned());
    out.join("\n")
}

/// Whether any of `secrets` is still in the redacted `text`.
fn shown(text: &str, secrets: &[String]) -> Vec<String> {
    let (out, _) = redact_private_key_blocks(text);
    secrets
        .iter()
        .filter(|secret| out.contains(secret.as_str()))
        .cloned()
        .collect()
}

#[test]
fn an_end_then_a_begin_on_one_line_leaves_the_next_key_open() {
    for separator in ["", " ", " and then "] {
        let second = body('b', 3);
        let tail = "C4k7/o9lrg==".to_owned();
        let mut lines = vec![format!(
            "-----END RSA PRIVATE KEY-----{separator}-----BEGIN RSA PRIVATE KEY-----"
        )];
        lines.extend_from_slice(&second);
        lines.push(tail.clone());
        lines.push("-----END RSA PRIVATE KEY-----".to_owned());
        lines.push("after the keys".to_owned());
        let text = format!(
            "{}\n{}\n",
            pem(&body('a', 2)).rsplit_once('\n').expect("two lines").0,
            lines.join("\n")
        );
        let mut secrets = second;
        secrets.push(tail);
        assert_eq!(
            shown(&text, &secrets),
            Vec::<String>::new(),
            "{separator:?}"
        );
        assert!(
            redact_private_key_blocks(&text)
                .0
                .contains("after the keys")
        );
    }
}

/// With no END after it, a line that opens the next key after closing one
/// hides that key's whole body.
#[test]
fn an_end_then_a_begin_opens_a_block_from_outside_too() {
    let secrets = body('c', 3);
    let text = format!(
        "first -----END RSA PRIVATE KEY----- x -----BEGIN RSA PRIVATE KEY-----\n{}\n",
        secrets.join("\n")
    );
    assert_eq!(shown(&text, &secrets), Vec::<String>::new());
}

/// The other order is one whole block on one line, as a JSON string holds it:
/// what follows is not a key.
#[test]
fn a_begin_then_an_end_on_one_line_stays_closed() {
    let text = "{\"k\":\"-----BEGIN PRIVATE KEY-----\\nQUJD\\n-----END PRIVATE KEY-----\\n\"}\nnext\nline two\n";
    let (out, changed) = redact_private_key_blocks(text);
    assert!(changed);
    assert_eq!(
        out.lines().skip(1).collect::<Vec<_>>(),
        vec!["next", "line two"]
    );
    assert_eq!(private_key_block_line_ranges(text), vec![(1, 1)]);
}

#[test]
fn a_line_that_is_not_key_text_does_not_end_a_body() {
    let lines = body('d', 6);
    let tail = "eYsq38TeMQ==".to_owned();
    let mut inner = lines[..2].to_vec();
    inner.push("# rotated key follows".to_owned());
    inner.extend_from_slice(&lines[2..]);
    inner.push(tail.clone());
    let terminated = pem(&inner);
    let unterminated = terminated
        .rsplit_once('\n')
        .expect("an END line")
        .0
        .to_owned();
    let mut secrets = lines;
    secrets.push(tail);
    for text in [terminated, unterminated] {
        assert_eq!(shown(&text, &secrets), Vec::<String>::new(), "{text}");
        assert!(
            redact_private_key_blocks(&text)
                .0
                .contains("# rotated key follows"),
            "the line holds no key text: {text}"
        );
    }
}

/// A line of prose straight after BEGIN does not end the block either: the
/// body that follows it is still hidden.
#[test]
fn a_line_before_the_first_body_line_does_not_end_the_block() {
    let lines = body('e', 3);
    let mut inner = vec!["this key is rotated weekly".to_owned()];
    inner.extend_from_slice(&lines);
    let text = pem(&inner);
    let open = text.rsplit_once('\n').expect("an END line").0;
    assert_eq!(shown(&text, &lines), Vec::<String>::new());
    assert_eq!(shown(open, &lines), Vec::<String>::new());
}

/// After a mention, only a line that reads as key text is hidden: armor-shaped
/// `name: value` lines hold no key bytes, however many of them follow.
#[test]
fn headers_after_a_mention_stay_readable() {
    let text = "rules:\n  - name: pem\n    pattern: \"-----BEGIN RSA PRIVATE KEY-----\"\n    severity: high\n    message: found a key\n    tags: secrets\n\n  - name: next\n    pattern: other value\n";
    let (out, _) = redact_private_key_blocks(text);
    for kept in [
        "severity: high",
        "message: found a key",
        "tags: secrets",
        "- name: next",
        "pattern: other value",
    ] {
        assert!(out.contains(kept), "{kept} hidden: {out}");
    }
    assert_eq!(out.lines().count(), text.lines().count());
    let rust = "Rule {\n    marker: \"-----BEGIN EC PRIVATE KEY-----\",\n    name: \"pem\",\n    severity: Severity::High,\n    enabled: true,\n}\n";
    let (out, _) = redact_private_key_blocks(rust);
    for kept in ["name: \"pem\"", "severity: Severity::High", "enabled: true"] {
        assert!(out.contains(kept), "{kept} hidden: {out}");
    }
}

/// A real armor header stays readable; the body after it, and a header that
/// carries a long key run, do not.
#[test]
fn armor_headers_hide_only_key_text() {
    let lines = body('f', 3);
    let mut inner = vec![
        "Proc-Type: 4,ENCRYPTED".to_owned(),
        "DEK-Info: AES-128-CBC,0123ABCD".to_owned(),
        String::new(),
    ];
    inner.extend_from_slice(&lines);
    let text = pem(&inner);
    let (out, _) = redact_private_key_blocks(&text);
    assert!(out.contains("Proc-Type: 4,ENCRYPTED"), "{out}");
    assert_eq!(shown(&text, &lines), Vec::<String>::new());
    let carried = format!("Comment: {}", lines[0]);
    let text = pem(&[carried, lines[1].clone()]);
    assert_eq!(shown(&text, &lines), Vec::<String>::new());
}

/// A separator made only of `=` or `/` is not a long key run: code after a
/// mention keeps its single-word lines.
#[test]
fn a_rule_of_punctuation_is_not_key_text() {
    let rule = "=".repeat(60);
    let text = format!("marker = \"-----BEGIN RSA PRIVATE KEY-----\"\nlet x = 1;\n{rule}\nNone,\n");
    let (out, _) = redact_private_key_blocks(&text);
    assert!(out.contains("None,"), "{out}");
}

#[test]
fn a_leading_plus_concatenation_is_quoting_not_text() {
    let lines = body('g', 2);
    let tail = "VdX8BNaFVg==".to_owned();
    let mut text = vec!["const k = \"-----BEGIN RSA PRIVATE KEY-----\\n\"".to_owned()];
    text.extend(lines.iter().map(|line| format!("  + \"{line}\\n\"")));
    text.push(format!("  + \"{tail}\\n\""));
    text.push("  + \"-----END RSA PRIVATE KEY-----\";".to_owned());
    text.push("const after = 1;".to_owned());
    let text = text.join("\n");
    let mut secrets = lines;
    secrets.push(tail);
    assert_eq!(shown(&text, &secrets), Vec::<String>::new());
    assert!(
        redact_private_key_blocks(&text)
            .0
            .contains("const after = 1;")
    );
}
