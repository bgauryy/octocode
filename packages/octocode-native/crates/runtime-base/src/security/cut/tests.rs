use super::*;

/// A row shows its own bytes with each run it reaches replaced once; a run
/// past the row's lines, or a row that is not the source's bytes at its
/// place (whatever its length), goes as the bare placeholder.
#[test]
fn a_rendered_row_replaces_exactly_its_runs() {
    let source = format!("{}blob: Qm # ghp_Zq7 tail", " ".repeat(102));
    let at = 102..102 + "blob: Qm # ghp_Z".len();
    let text = &source[at.clone()];
    let line = 0..source.len();
    let from = |r: Range<usize>| source.get(r);
    let render = |runs: &[(Range<usize>, &str)], text: &str, bound: &Range<usize>| {
        rendered(runs.iter().cloned(), text, &at, bound, from)
    };
    assert_eq!(
        render(&[(108..110, "[A]"), (113..120, "[B]")], text, &line),
        "blob: [A] # [B]"
    );
    assert_eq!(
        render(&[(108..112, "")], text, &line),
        "blob: [REDACTED] ghp_Z"
    );
    assert_eq!(render(&[(90..200, "[A]")], text, &(100..150)), REDACTED);
    assert_eq!(render(&[(0..1, "[A]")], text, &line), text);
    let forged = "blob: Qm # xxx_Z";
    assert_eq!(render(&[(108..110, "[A]")], forged, &line), REDACTED);
    assert_eq!(
        rendered([(108..110, "[A]")], text, &(0..3), &line, from),
        REDACTED
    );
    let unicode = "\u{fc}\u{b7}\u{e9}";
    assert_eq!(
        rendered(
            [(1..2, "[A]")],
            unicode,
            &(0..unicode.len()),
            &(0..9),
            |r| unicode.get(r)
        ),
        "[A]\u{b7}\u{e9}"
    );
}

/// A run that reaches past the row's lines shows as the placeholders its
/// replacement holds, each once and nothing between them: the kind a
/// read of its lines shows, on whichever of its lines the row is.
#[test]
fn a_run_past_the_rows_lines_keeps_its_kind() {
    let source = "a = \"0123\" \\\n    \"4567\"\nb = 1";
    let run = source.find("0123").expect("start")..source.find("4567").expect("end") + 4;
    let from = |r: Range<usize>| source.get(r);
    let eol = source.find('\n').expect("eol");
    let lines = [0..eol, eol + 1..source.rfind('\n').expect("eol")];
    let render = |replacement: &str, literal: &str, bound: &Range<usize>| {
        let start = source.find(literal).expect("literal");
        let at = start..start + literal.len();
        rendered([(run.clone(), replacement)], literal, &at, bound, from)
    };
    let kind = "[REDACTED-ENVVARSECRETS]";
    let broken = format!("{kind}\n");
    assert_eq!(render(&broken, "\"0123\"", &lines[0]), format!("\"{kind}"));
    assert_eq!(render(&broken, "\"4567\"", &lines[1]), format!("{kind}\""));
    // Within the row's lines the replacement stands as the pass wrote it.
    assert_eq!(
        render(&broken, "\"0123\"", &(0..lines[1].end)),
        format!("\"{broken}")
    );
    // Text of other lines between two placeholders stays out.
    let two = format!("{kind} kept\n[REDACTED-B]{kind}");
    assert_eq!(
        render(&two, "\"4567\"", &lines[1]),
        format!("{kind}[REDACTED-B]\"")
    );
    let failed = octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER;
    assert_eq!(
        render(&format!("{failed}\n{failed}"), "\"4567\"", &lines[1]),
        format!("{failed}\"")
    );
    assert_eq!(
        render("other text", "\"4567\"", &lines[1]),
        format!("{REDACTED}\"")
    );
}

const SECRET: &str = "ghp_Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7Zq7";

fn redacted(source: &str) -> Edits {
    let sanitized =
        octocode_engine::security::sanitizer::sanitize_content(source).expect("sanitized");
    assert!(sanitized.has_secrets, "{source}");
    sanitized.edits
}

fn span(line: &str, offset: usize, len: usize) -> Range<usize> {
    placed(
        &[line],
        0,
        &Cut::Span {
            offset,
            text: &line[offset..offset + len],
        },
    )
}

/// A one-character cut inside a secret shows part of it, though the same
/// character occurs outside the secret on the same line.
#[test]
fn a_one_character_cut_of_a_secret_is_exposed_wherever_its_text_recurs() {
    let line = format!("Zq7 g h p _ let token = \"{SECRET}\"; Zq7");
    let edits = redacted(&line);
    let start = line.find(SECRET).expect("secret");
    for at in start..start + SECRET.len() {
        assert!(line[..start].contains(&line[at..=at]));
        assert!(exposes(&edits, &span(&line, at, 1)), "byte {at}");
    }
    // Three characters that recur outside the secret, cut from inside it.
    assert!(line[..start].contains(&line[start + 4..start + 7]));
    assert!(exposes(&edits, &span(&line, start + 4, 3)));
}

/// A cut wholly outside every redaction shows none.
#[test]
fn a_cut_outside_every_redaction_is_not_exposed() {
    let line = format!("let token = \"{SECRET}\"; let next = 1;");
    let edits = redacted(&line);
    let start = line.find(SECRET).expect("secret");
    assert!(!exposes(&edits, &span(&line, 0, 3)));
    assert!(!exposes(&edits, &span(&line, 0, start)));
    let after = start + SECRET.len();
    assert!(!exposes(&edits, &span(&line, after, line.len() - after)));
}

/// A cut that straddles a redaction's edge shows its part of the secret.
#[test]
fn a_cut_across_a_redaction_edge_is_exposed() {
    let line = format!("let token = \"{SECRET}\";");
    let edits = redacted(&line);
    let start = line.find(SECRET).expect("secret");
    let end = start + SECRET.len();
    assert!(exposes(&edits, &span(&line, start - 2, 3)));
    assert!(exposes(&edits, &span(&line, end - 1, 2)));
    assert!(!exposes(&edits, &span(&line, end, 2)));
    assert!(exposes(&edits, &span(&line, 0, line.len())));
}

/// Positions are bytes of multi-byte text: a cut of a character beside a
/// secret stays shown, one of the secret's own does not.
#[test]
fn unicode_cuts_are_judged_by_byte_position() {
    let line = format!("éé token = \"{SECRET}\" · ππ Zq7");
    let edits = redacted(&line);
    let start = line.find(SECRET).expect("secret");
    for (at, c) in line.char_indices() {
        let inside = (start..start + SECRET.len()).contains(&at);
        assert_eq!(
            exposes(&edits, &span(&line, at, c.len_utf8())),
            inside,
            "{c:?} at {at}"
        );
    }
}

/// A row is placed on its line of a window; a span that is not there
/// takes its lines whole, and a lines row its lines.
#[test]
fn rows_are_placed_in_their_window() {
    let lines = ["ab", "céd", "ef"];
    let cut = Cut::Span {
        offset: 1,
        text: "é",
    };
    assert_eq!(placed(&lines, 1, &cut), 4..6);
    let cut = Cut::Span {
        offset: 1,
        text: "éd\nef",
    };
    assert_eq!(placed(&lines, 1, &cut), 4..10);
    let cut = Cut::Span {
        offset: 0,
        text: "zz",
    };
    assert_eq!(placed(&lines, 1, &cut), 3..7);
    assert_eq!(placed(&lines, 0, &Cut::Lines(2)), 0..7);
    assert_eq!(placed(&lines, 2, &Cut::Lines(5)), 8..10);
}
