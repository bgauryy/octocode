//! Cuts by position of sources whose redaction spans lines: a position is a
//! place in the source as read, and only the bytes the pass replaced are
//! hidden. All secrets are fake.
use super::*;

/// Text no detector reads as a word: `len` characters of a fixed sequence
/// seeded by `seed`.
fn body(seed: u64, len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let at = usize::try_from(state >> 33).unwrap_or(0) % ALPHABET.len();
            char::from(ALPHABET[at])
        })
        .collect()
}

fn judge(source: &str, name: &str) -> JudgedText {
    JudgedText::judge(
        source,
        &Path::new("/w/src").join(name),
        &ContentSecurity::new(),
    )
}

/// The 1-based line and 0-based byte column of source byte `at`.
fn position(source: &str, at: usize) -> (usize, usize) {
    let before = &source[..at];
    (
        before.matches('\n').count() + 1,
        at - before.rfind('\n').map_or(0, |eol| eol + 1),
    )
}

/// A key block inside a string that spans lines, as each language writes
/// one: the file, what stands before the key on its first line and after it
/// on its last, and the declared name.
const KEY_BLOCKS: [(&str, &str, &str, &str); 4] = [
    ("k.ts", "const pem = `", "`;", "pem"),
    ("k.py", "KEY = \"\"\"", "\"\"\"", "KEY"),
    ("k.go", "var pemKey = `", "`", "pemKey"),
    ("k.rs", "    let key_text = \"", "\";", "key_text"),
];

/// The key pass hides a key's bytes and no other: what a line holds before
/// the key and after it, the declared name first, and the block's armor are
/// the source's own text at its place, wherever the block stands in its file.
#[test]
fn a_key_block_hides_its_bytes_and_leaves_the_code_around_them() {
    let fillers = ["", "// head\n", "const before = 1;\nconst other = 2;\n"];
    for (seed, (file, lead, tail, name)) in (1u64..).zip(KEY_BLOCKS) {
        for (before, after) in fillers.iter().flat_map(|a| fillers.map(|b| (*a, b))) {
            let (b1, b2) = (body(seed, 64), body(seed + 40, 64));
            let source = format!(
                "{before}{lead}-----BEGIN RSA PRIVATE KEY-----\n{b1}\n{b2}\n-----END RSA PRIVATE KEY-----{tail}\n{after}"
            );
            let judged = judge(&source, file);
            let layout = format!("{file} after {before:?}");
            let first = before.matches('\n').count() + 1;
            let read = judged.lines(first, 4).expect("the block's lines");
            assert!(
                !read.contains(&b1) && !read.contains(&b2) && read.contains("[REDACTED-"),
                "{layout}: {read}"
            );
            assert!(
                read.starts_with(lead) && read.ends_with(tail),
                "{layout}: {read}"
            );

            let at = before.len() + lead.find(name).expect("name");
            let named = at..at + name.len();
            assert!(!judged.exposes(&named), "{layout}: the name is no key byte");
            assert_eq!(judged.cut(named.clone()), name, "{layout}");
            let (line, column) = position(&source, at);
            assert_eq!(
                judged.fragment(line, Some(column), name),
                (name.into(), true),
                "{layout}"
            );
            let led = before.len()..before.len() + lead.len();
            assert_eq!(judged.cut(led), lead, "{layout}");
            let end = source.len() - after.len() - 1;
            assert_eq!(judged.cut(end - tail.len()..end), tail, "{layout}");
            // The armor reads as written; every byte of the body is hidden,
            // each line under one placeholder.
            const BEGIN: &str = "-----BEGIN RSA PRIVATE KEY-----";
            const END: &str = "-----END RSA PRIVATE KEY-----";
            let opened = before.len() + lead.len();
            assert_eq!(judged.cut(opened..opened + BEGIN.len()), BEGIN, "{layout}");
            let closed = end - tail.len();
            assert_eq!(judged.cut(closed - END.len()..closed), END, "{layout}");
            let key = opened + BEGIN.len() + 1..closed - END.len() - 1;
            for at in key.clone().filter(|at| source.as_bytes()[*at] != b'\n') {
                assert!(judged.exposes(&(at..at + 1)), "{layout}: byte {at}");
            }
            let shown = judged.cut(key.start + 3..key.end - 3);
            assert!(
                !shown.contains(&b1[..8]) && !shown.contains("[REDACTED]"),
                "{layout}: {shown}"
            );
        }
    }
}

/// A column names one place: the bytes there, in whatever unit the tool
/// counts, and never another occurrence of the same text. Without a column
/// an occurrence the pass replaced part of decides.
#[test]
fn a_column_places_a_fragment_at_its_bytes_and_nowhere_else() {
    let value = format!("{}f", body(3, 24));
    for lead in ["", "/* é· */ ", "var é = \"ππ\"; "] {
        let source = format!("{lead}func f() {{ secret := \"{value}\"; _ = secret }}\n");
        let judged = judge(&source, "one.go");
        let secret = source.find(&value).expect("value");
        assert!(judged.exposes(&(secret..secret + value.len())), "{source}");
        let at = lead.len() + "func ".len();
        let head = &source[..at];
        // Bytes, characters and UTF-16 units each name the same place.
        for column in [
            head.len(),
            head.chars().count(),
            head.encode_utf16().count(),
        ] {
            assert_eq!(
                judged.place(1, Some(column), "f"),
                Some(at..at + 1),
                "{lead:?}"
            );
            assert_eq!(
                judged.fragment(1, Some(column), "f"),
                ("f".into(), true),
                "{lead:?} column {column}"
            );
        }
        // No `f` starts two columns on: the column is not searched from.
        // (In no unit: bytes, characters and UTF-16 units of it are no `f`.)
        let off = head.len() + 2;
        assert_eq!(judged.place(1, Some(off), "f"), None, "{lead:?}");
        let (shown, placed) = judged.fragment(1, Some(off), "f");
        assert!(
            placed && shown == judged.lines(1, 1).expect("line"),
            "{shown}"
        );
        // A value fragment with no column is the hidden occurrence.
        let unplaced = judged.place(1, None, "f").expect("an occurrence");
        assert!(judged.exposes(&unplaced), "{lead:?}");
    }
}

/// A cut inside a value hidden across lines shows that value's placeholder,
/// the one a read of its lines shows, on whichever of its lines the cut is.
#[test]
fn a_cut_of_one_line_of_a_value_hidden_across_lines_keeps_its_placeholder() {
    let tail = body(5, 36);
    let source = format!("label = 1\ntoken = \"ghp_\" \\\n    \"{tail}\"\nafter = 2\n");
    let judged = judge(&source, "s.py");
    let read = judged.lines(2, 2).expect("both lines");
    assert!(!read.contains(&tail), "fixture no longer hidden: {read}");
    let placeholder = octocode_engine::security::placeholder_regex()
        .find(&read)
        .expect("a placeholder of a kind")
        .as_str()
        .to_owned();
    for literal in ["\"ghp_\"".to_owned(), format!("\"{tail}\"")] {
        let at = source.find(&literal).expect("literal");
        let shown = judged.cut(at..at + literal.len());
        assert!(
            shown.contains(&placeholder) && !shown.contains("[REDACTED]"),
            "{literal}: {shown}, the read: {read}"
        );
        assert!(
            !shown.contains(&tail[..8]) && !shown.contains('\n'),
            "{shown}"
        );
    }
    // The lines around it are the source's.
    assert_eq!(judged.lines(1, 1).expect("line 1"), "label = 1");
    assert_eq!(judged.lines(4, 1).expect("line 4"), "after = 2");
}
