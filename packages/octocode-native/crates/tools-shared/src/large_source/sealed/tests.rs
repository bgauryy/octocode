use super::*;
use octocode_runtime_base::security::ContentSecurity;

/// `raw` with `edits` applied.
fn apply(raw: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    for edit in edits {
        assert!(
            edit.range.start >= at,
            "edits are sorted and apart: {edits:?}"
        );
        out.extend_from_slice(&raw[at..edit.range.start]);
        out.extend_from_slice(edit.text.as_bytes());
        at = edit.range.end;
    }
    out.extend_from_slice(&raw[at..]);
    out
}

fn record(raw: &[u8], limits: Limits, piece: usize) -> Vec<Edit> {
    let scan = ContentSecurity::new();
    let mut recorder = Recorder::new(
        Path::new("/w/big.js"),
        limits,
        super::super::memory().reserve(0).expect("room"),
    );
    for chunk in raw.chunks(piece) {
        recorder.feed(chunk, &scan, NO_KEYS);
    }
    recorder.finish(&scan, NO_KEYS).expect("recorded").0
}

/// A source no key block replaces a line of, every line of it closed.
const NO_KEYS: KeyLines<'static> = KeyLines {
    lines: &[],
    closed: usize::MAX,
};

fn whole(raw: &str) -> String {
    let scan = ContentSecurity::new();
    scan.redact_source_lines(raw, Path::new("/w/big.js"))
        .expect("scanned")
        .0
        .into_owned()
}

const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

fn long_bearer(len: usize) -> String {
    let body: String = (0..len)
        .map(|at| {
            char::from(
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"[at * 7 % 62],
            )
        })
        .collect();
    format!("Authorization: Bearer {body}")
}

/// The edits a streamed pass records seal the source as a whole-source scan
/// does, whatever the chunk and piece sizes, a redaction far longer than a
/// chunk and its overlap included.
#[test]
fn recorded_edits_seal_the_source_as_a_whole_scan_does() {
    let filler = "call(a1, \"x\");".repeat(40);
    let mut text = String::new();
    for round in 0..6 {
        text.push_str(&filler);
        text.push_str(&format!(" t{round} = \"{TOKEN}\"; "));
        if round == 2 {
            text.push_str(&format!("h=\"{}\";", long_bearer(5_000)));
        }
        if round % 2 == 1 {
            text.push('\n');
        }
    }
    let expected = whole(&text);
    assert!(
        !expected.contains(TOKEN) && expected.contains("[REDACTED"),
        "{expected}"
    );
    for limits in [
        Limits {
            chunk: 64,
            overlap: 64,
        },
        Limits {
            chunk: 700,
            overlap: 100,
        },
        Limits::REAL,
    ] {
        for piece in [1, 7, 4096] {
            let edits = record(text.as_bytes(), limits, piece);
            let sealed = String::from_utf8(apply(text.as_bytes(), &edits)).expect("utf8");
            assert_eq!(sealed, expected, "{limits:?} piece {piece}");
        }
    }
}

/// Invalid UTF-8 shifts no edit off the source bytes it hides.
#[test]
fn edits_name_source_bytes_past_invalid_utf8() {
    let mut raw = b"a\xff\xfe b = \"".to_vec();
    raw.extend_from_slice(TOKEN.as_bytes());
    raw.extend_from_slice(b"\";\n\xc3 c = 1\n");
    for piece in [1, 3, 100] {
        let edits = record(
            &raw,
            Limits {
                chunk: 4,
                overlap: 64,
            },
            piece,
        );
        assert_eq!(edits.len(), 1, "{edits:?}");
        let at = raw
            .windows(TOKEN.len())
            .position(|w| w == TOKEN.as_bytes())
            .expect("token");
        assert!(
            edits[0].range.start <= at && edits[0].range.end >= at + TOKEN.len(),
            "{edits:?}"
        );
        assert!(
            !apply(&raw, &edits)
                .windows(TOKEN.len())
                .any(|w| w == TOKEN.as_bytes())
        );
    }
}

/// Each placeholder stands for its own bytes; text between them stays out of
/// every edit.
#[test]
fn placeholders_land_on_their_own_bytes() {
    let raw = format!("x = \"{TOKEN}\"; keep = 1; y = \"{TOKEN}\";\n");
    let clean = whole(&raw);
    let edits = align(&raw, &clean);
    assert_eq!(edits.len(), 2, "{edits:?}");
    for (range, text) in &edits {
        assert!(
            raw[range.clone()].contains(TOKEN) || TOKEN.contains(&raw[range.clone()]),
            "{range:?}"
        );
        assert!(text.starts_with("[REDACTED"), "{text}");
        assert!(!raw[range.clone()].contains("keep"));
    }
}

/// A sealed offset maps back to the keyed offset of the same text, past edits
/// and private-key lines; inside a placeholder it is its edit's start.
#[test]
fn sealed_offsets_map_to_keyed_offsets() {
    let edit = |range: Range<usize>, text: &str| Edit {
        range,
        text: text.into(),
    };
    // Source from byte 10: 5 plain, edit 15..115 as "[R]", 10 plain, key line
    // 125..1125 shown in 7 bytes, then plain.
    let edits = [edit(15..115, "[R]")];
    let keys = [(125..1125, 7)];
    assert_eq!(keyed_offset(&edits, &keys, 10, 0), Some(0));
    assert_eq!(keyed_offset(&edits, &keys, 10, 5), Some(5));
    assert_eq!(
        keyed_offset(&edits, &keys, 10, 6),
        Some(5),
        "inside the placeholder"
    );
    assert_eq!(
        keyed_offset(&edits, &keys, 10, 8),
        Some(105),
        "just past it"
    );
    assert_eq!(
        keyed_offset(&edits, &keys, 10, 18),
        Some(115),
        "at the key line"
    );
    assert_eq!(
        keyed_offset(&edits, &keys, 10, 25),
        Some(122),
        "past the key line"
    );
    let inside = shift(&edits, 10..2000);
    assert_eq!(inside.removed - inside.added, 97);
    assert_eq!(inside.apply(1990), Some(1990 - 97));
    assert_eq!(shift(&edits, 116..2000), Shift::default());
}

/// A sealed length counts each placeholder's own bytes: one longer than its
/// source (a short structural value) grows the view, one shorter shrinks it,
/// and the two never cancel into a wrong total (review: 16 source bytes
/// sealed to 40 were reported as 16).
#[test]
fn a_shift_counts_growing_and_shrinking_placeholders() {
    let edit = |range: Range<usize>, text: &str| Edit {
        range,
        text: text.into(),
    };
    let seal = |raw: &str, edits: &[Edit]| {
        let mut out = String::new();
        let mut at = 0;
        for edit in edits {
            out.push_str(raw.get(at..edit.range.start).expect("plain"));
            out.push_str(&edit.text);
            at = edit.range.end;
        }
        out + raw.get(at..).expect("tail")
    };
    let grow = "[REDACTED-KUBERNETESSECRETJSON]";
    let raw = "X \"a\":\"b\" YYYYY\n";
    let edits = [edit(2..9, grow)];
    assert_eq!(
        shift(&edits, 0..raw.len()).apply(raw.len()),
        Some(seal(raw, &edits).len())
    );
    // Mixed: a growing and a shrinking edit, then a multi-byte tail.
    let raw = format!("X \"a\":\"b\" {} Z é日🦀\n", "t".repeat(40));
    let edits = [edit(2..9, grow), edit(10..50, "[REDACTED-TOKEN]")];
    let sealed = seal(&raw, &edits);
    assert_eq!(
        shift(&edits, 0..raw.len()).apply(raw.len()),
        Some(sealed.len())
    );
    // A span after the growing edit: only the edits inside it count.
    assert_eq!(
        shift(&edits, 10..raw.len()).apply(raw.len() - 10),
        Some(sealed.len() - 2 - grow.len() - 1)
    );
    assert_eq!(shift(&edits, 50..raw.len()), Shift::default());
}

/// Offsets from a record that breaks its own order are refused, never
/// clamped into a plausible wrong place (review B33 finding 3): an edit that
/// removes more than the span it was counted in holds, and an edit that
/// starts inside a key line.
#[test]
fn a_broken_record_places_no_offset() {
    let edit = |range: Range<usize>, text: &str| Edit {
        range,
        text: text.into(),
    };
    let edits = [edit(0..40, "[R]")];
    assert_eq!(shift(&edits, 0..40).apply(10), None, "40 removed of 10");
    let edits = [edit(20..30, "[R]")];
    let keys = [(10..50, 7)];
    assert_eq!(keyed_offset(&edits, &keys, 0, 30), None);
}

/// B289: one line far longer than a chunk and its overlap, holding many
/// secrets, seals as a whole-source scan seals it: the scanner's judgement
/// does not depend on which chunk holds a value.
#[test]
fn a_long_line_of_secrets_seals_as_a_whole_scan_does() {
    let mut text = String::from("Cookie: ");
    let mut n = 0;
    while text.len() < 3 * Limits::REAL.chunk {
        text.push_str(&format!("JSESSIONID=v{n:06}; "));
        n += 1;
    }
    text.push_str("end\n");
    let expected = whole(&text);
    assert!(
        !expected.contains("\"v0"),
        "the whole scan redacts every value"
    );
    for piece in [4096, 65536] {
        let edits = record(text.as_bytes(), Limits::REAL, piece);
        let sealed = String::from_utf8(apply(text.as_bytes(), &edits)).expect("utf8");
        assert_eq!(sealed, expected, "piece {piece}");
    }
}

/// A private-key line is judged as the placeholder the pass shows of it, and
/// a line still open waits for its close: a block's marker, closed or cut by
/// a judgement's end, names no base64 line before it a key's.
#[test]
fn a_key_marker_hides_no_line_before_its_block() {
    let innocent = "QUJDREVGR0hJSktMTU5PUFFSU1RVinnocentB64VldYWVowMTIzNDU2Nzg5\n";
    let filler = "let filler_value = 0;\n".repeat(12);
    let marker = "-----BEGIN OPENSSH PRIVATE KEY-----\n";
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n";
    let head = format!("{innocent}{filler}");
    let text = format!("{head}{marker}{body}");
    let placeholder = octocode_runtime_base::security::key_fragment_placeholder().len() + 1;
    let key_lines = [
        (head.len()..head.len() + marker.len(), placeholder),
        (head.len() + marker.len()..text.len(), placeholder),
    ];
    let limits = Limits {
        chunk: 64,
        overlap: 64,
    };
    let scan = ContentSecurity::new();
    // The marker's line is open at each cut inside it, then closed.
    for cut in [head.len() + 5, head.len() + marker.len() - 1] {
        let mut recorder = Recorder::new(
            Path::new("/w/big.txt"),
            limits,
            super::super::memory().reserve(0).expect("room"),
        );
        let open = KeyLines {
            lines: &[],
            closed: head.len(),
        };
        recorder.feed(&text.as_bytes()[..cut], &scan, open);
        let closed = KeyLines {
            lines: &key_lines,
            closed: text.len(),
        };
        recorder.feed(&text.as_bytes()[cut..], &scan, closed);
        let edits = recorder.finish(&scan, closed).expect("recorded").0;
        let outside = outside_key_lines(edits, &key_lines);
        assert_eq!(outside, [], "cut at {cut}");
    }
}
