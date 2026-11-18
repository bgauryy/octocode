use super::*;

const LINES: [&str; 7] = [
    "const s = \"İéxẞ\";",
    "DİYARBAKIR İstanbul ı I",
    "STRAẞE straße ẞß",
    "e\u{301}cole Ё\u{308} a\u{30a}\u{301}",
    "👍🏽 👩‍👩‍👧 ΟΔΟΣ Σ σ",
    "ﬁle ǅ Ω K Å",
    "+ let x = \"🎉 İ\"; // ẞ",
];

/// Every char's lowercase byte offset maps back to that char, whatever
/// lowercasing does to the line's bytes: `İ` grows, `ẞ` shrinks (and
/// together keep the line's length), combining marks, emoji and a
/// final sigma keep theirs.
#[test]
fn lowercase_offsets_map_back_to_their_chars_in_any_script() {
    for line in LINES {
        let folded = Folded::of(line);
        let lower = folded.text();
        let mut at = 0;
        for (index, (start, c)) in line.char_indices().enumerate() {
            assert!(lower.is_char_boundary(at), "{line:?} at {at}");
            assert_eq!(folded.char_index(at), index, "{line:?} at {at}");
            assert_eq!(folded.origin(at), start..start + c.len_utf8(), "{line:?}");
            // An offset inside the char's lowercase form is that char's.
            let width: usize = c.to_lowercase().map(char::len_utf8).sum();
            assert_eq!(folded.char_index(at + width - 1), index, "{line:?}");
            at += width;
        }
        assert_eq!(at, lower.len(), "{line:?}");
        assert_eq!(folded.char_index(at), line.chars().count());
        assert_eq!(folded.origin(at), line.len()..line.len());
    }
}

/// A match found in the lowercase text covers exactly the original
/// chars it lowercased from, wherever `İ`/`ẞ` shift the bytes.
#[test]
fn a_lowercase_match_spans_its_original_chars() {
    for (line, needle, original) in [
        ("İéxẞ needle İ", "needle", "needle"),
        ("İéxẞ", "i\u{307}éxß", "İéxẞ"),
        ("DİYARBAKIR", "di\u{307}yarbakir", "DİYARBAKIR"),
        ("ΟΔΟΣ x", "οδος", "ΟΔΟΣ"),
        ("STRAẞE", "straße", "STRAẞE"),
        ("👍🏽 ÉCOLE", "école", "ÉCOLE"),
    ] {
        let folded = Folded::of(line);
        let at = folded.text().find(needle).expect(needle);
        assert_eq!(
            &line[folded.span(at..at + needle.len())],
            original,
            "{line:?}"
        );
    }
}

/// One folded copy serves every case-insensitive pattern, and each span
/// slices the source's own bytes, around length-changing chars too.
#[test]
fn one_fold_serves_every_pattern_in_source_coordinates() {
    let source = "İneedle and İNEEDLE, then Straße STRASSE";
    let folded = Folded::of(source);
    let text = |spans: Vec<Range<usize>>| {
        spans
            .into_iter()
            .map(|span| &source[span])
            .collect::<Vec<_>>()
    };
    assert_eq!(text(folded.ranges("needle")), ["needle", "NEEDLE"]);
    assert_eq!(text(folded.ranges("İ")), ["İ", "İ"]);
    assert_eq!(text(folded.ranges("straße")), ["Straße"]);
    assert_eq!(text(folded.ranges("THEN")), ["then"]);
    assert!(folded.ranges("").is_empty());
}

/// The ASCII path holds exactly what the per-char path computes, and every
/// offset and span maps back the same.
fn assert_same_as_char_by_char(source: &str) {
    let (fast, full) = (Folded::of(source), Folded::char_by_char(source));
    assert_eq!(fast.text, full.text, "{source:?}");
    assert_eq!(fast.changes, full.changes, "{source:?}");
    assert!(fast.changes.is_empty(), "{source:?}");
    for at in 0..=source.len() + 1 {
        assert_eq!(fast.origin(at), full.origin(at), "{source:?} at {at}");
        for end in at..=source.len() + 1 {
            assert_eq!(fast.span(at..end), full.span(at..end), "{source:?}");
        }
    }
    for pattern in ["a", "A", "z\n", "\u{212a}", "\u{130}", "stra\u{df}e", ""] {
        assert_eq!(fast.ranges(pattern), full.ranges(pattern), "{source:?}");
    }
}

#[test]
fn the_ascii_path_maps_every_ascii_char_as_the_char_path_does() {
    let every: String = (0..=127u8).map(char::from).collect();
    assert_same_as_char_by_char(&every);
    assert_same_as_char_by_char("");
    assert_same_as_char_by_char("MiXeD\r\nCASE\n\0");
}

/// One char past ASCII anywhere keeps the per-char map: lowercasing it
/// changes its width, and every offset after it shifts.
#[test]
fn one_non_ascii_char_keeps_the_per_char_map() {
    for source in ["\u{130}ABC", "AB\u{130}C", "ABC\u{130}", "ABC\u{1e9e}D"] {
        let folded = Folded::of(source);
        assert_eq!(folded.text, source.to_lowercase(), "{source:?}");
        assert_eq!(folded.changes.len(), 1, "{source:?}");
        assert_eq!(folded.changes, Folded::char_by_char(source).changes);
    }
}

proptest::proptest! {
    #[test]
    fn any_ascii_text_folds_as_the_char_path_does(source in "[\\x00-\\x7f]{0,48}") {
        assert_same_as_char_by_char(&source);
    }
}
