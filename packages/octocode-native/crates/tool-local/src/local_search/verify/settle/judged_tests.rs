use super::*;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn verdict(n: usize) -> (WindowKey, WindowJudgment) {
    let key = (
        std::path::PathBuf::from(format!("src/{n}.rs")),
        [0; 32],
        [u8::try_from(n).expect("small"); 32],
    );
    let edits = Edits::of_pass([(0..n + 1, 10)]);
    (key, (Arc::new(edits), Arc::from("x".repeat(n * 7))))
}

/// Each verdict learned after the bill is set is charged its own bytes,
/// once, and what is kept weighs exactly what was charged.
#[test]
fn learned_verdicts_are_charged_their_real_size() {
    let judged = JudgedWindows::default();
    let charged = Arc::new(AtomicUsize::new(0));
    let total = Arc::clone(&charged);
    judged.billing().set(Arc::new(move |bytes| {
        total.fetch_add(bytes, Ordering::SeqCst);
        true
    }));
    let mut expected = 0;
    for n in 1..=5 {
        let (key, judgment) = verdict(n);
        expected += verdict_weight(&key, &judgment);
        judged.learn(key.clone(), judgment.clone());
        judged.learn(key, judgment);
    }
    assert_eq!(charged.load(Ordering::SeqCst), expected);
    assert_eq!(judged.weight(), expected);
}

fn placed_spans(rows: &[octocode_engine::types::TextSearchMatch]) -> Vec<Option<RowLines>> {
    rows.iter()
        .map(|row| row_lines(RowShape::Span, row))
        .collect()
}

fn read_of(line: &str) -> VerificationLines {
    VerificationLines {
        key_ranges: Vec::new(),
        retained: BTreeMap::from([(1, line.to_owned())]),
        retained_bytes: line.len(),
        lines_read: 1,
        unclassified: false,
        whole_retained: true,
        long_lines: std::collections::BTreeSet::new(),
    }
}

fn settled(line: &str, span: Range<usize>, known: &JudgedWindows) -> String {
    let value = &line[span.clone()];
    let mut rows = [octocode_engine::types::TextSearchMatch {
        deferred: None,
        line: 1,
        first_line: 1,
        last_line: None,
        column: u32::try_from(span.start + 1).expect("column"),
        byte_offset: u32::try_from(span.start).expect("offset"),
        value: value.into(),
        value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
        count: None,
        rank: None,
        window_start: None,
    }];
    let read = read_of(line);
    let judged = Judged {
        read: &read,
        whole: None,
    };
    let placed = placed_spans(&rows);
    settle_rows(
        &mut rows,
        &[(1, 1)],
        &judged,
        std::path::Path::new("/src/a.txt"),
        &ContentSecurity::new(),
        (&placed, RowShape::Span, Some((known, [7; 32]))),
        &HiddenRows::default(),
    );
    rows[0].value.clone()
}

/// A window's verdict is reused only for the exact text it judged: two
/// texts of the same lines and source bytes are judged each by their own
/// scan.
#[test]
fn a_verdict_is_never_replayed_onto_other_text_of_its_window() {
    let token = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";
    let known = JudgedWindows::default();
    let masked = "k = [REDACTED-GITHUBTOKENS] and a tail of text";
    assert_eq!(settled(masked, 4..10, &known), "[REDAC");
    let raw = format!("k = {token} and a tail of text");
    let shown = settled(&raw, 4..10, &known);
    assert!(!shown.contains("Z9"), "{shown}");
    assert_eq!(known.lock().len(), 2, "one verdict per judged text");
}

/// A span from a key block's armor onto the line after it, in a source no
/// whole scan could judge, shows nothing of that line: the token's first
/// bytes there match no pattern alone.
#[test]
fn a_span_past_a_key_block_without_a_whole_scan_goes_whole() {
    let lines = [
        "-----BEGIN RSA PRIVATE KEY-----",
        "MIIEowIBAAKCAQEA",
        "-----END RSA PRIVATE KEY-----",
        "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2",
    ];
    let mut read = read_of(lines[0]);
    read.retained = (1..=4).map(|n| (n, lines[n - 1].to_owned())).collect();
    read.lines_read = 4;
    read.key_ranges = vec![(1, 3)];
    let value = "KEY-----\nghp_Z9Q8";
    let mut rows = [octocode_engine::types::TextSearchMatch {
        deferred: None,
        line: 3,
        first_line: 3,
        last_line: Some(4),
        column: 22,
        byte_offset: 21,
        value: value.into(),
        value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
        count: None,
        rank: None,
        window_start: None,
    }];
    let judged = Judged {
        read: &read,
        whole: None,
    };
    let placed = placed_spans(&rows);
    settle_rows(
        &mut rows,
        &[(2, 5)],
        &judged,
        std::path::Path::new("/src/key.txt"),
        &ContentSecurity::new(),
        (&placed, RowShape::Span, None),
        &HiddenRows::default(),
    );
    assert!(!rows[0].value.contains("Z9"), "{}", rows[0].value);
}

const TOKEN: &str = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";

/// `value`, a matchOnly row at `offset` of line 1 of `line`, as settled.
fn settle_one(line: &str, value: &str, offset: usize) -> String {
    let mut rows = [octocode_engine::types::TextSearchMatch {
        deferred: None,
        line: 1,
        first_line: 1,
        last_line: None,
        column: u32::try_from(offset + 1).expect("column"),
        byte_offset: u32::try_from(offset).expect("offset"),
        value: value.into(),
        value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
        count: None,
        rank: None,
        window_start: None,
    }];
    let read = read_of(line);
    let judged = Judged {
        read: &read,
        whole: None,
    };
    let placed = placed_spans(&rows);
    settle_rows(
        &mut rows,
        &[(1, 1)],
        &judged,
        std::path::Path::new("/src/a.txt"),
        &ContentSecurity::new(),
        (&placed, RowShape::Span, None),
        &HiddenRows::default(),
    );
    rows[0].value.clone()
}

/// A row whose text is not its line's bytes at its place is not shown by
/// its length: equal lengths do not make it the source's bytes.
#[test]
fn a_row_of_the_wrong_bytes_goes_whole_even_at_the_right_length() {
    let line = format!("ab {TOKEN}");
    let value = "x".repeat(line.len());
    assert_eq!(settle_one(&line, &value, 0), REDACTED);
}

/// A span beside a secret on its line is shown exactly; a span of the
/// secret goes.
#[test]
fn a_span_beside_a_secret_is_shown_exactly() {
    let line = format!("aa {TOKEN} bb cc");
    let at = line.find("bb").expect("bb");
    assert_eq!(settle_one(&line, "bb", at), "bb");
    let at = line.find("Z9").expect("Z9");
    let shown = settle_one(&line, "Z9", at);
    assert!(
        shown.starts_with("[REDACTED") && !shown.contains("Z9"),
        "{shown}"
    );
}

/// A store that refuses the bytes keeps no verdict: it is judged again.
#[test]
fn a_refused_charge_keeps_no_verdict() {
    let judged = JudgedWindows::default();
    judged.billing().set(Arc::new(|_| false));
    let (key, judgment) = verdict(3);
    judged.learn(key.clone(), judgment);
    assert_eq!(judged.weight(), 0);
    assert!(judged.lock().get(&key).is_none());
}
