use super::line_count;

fn assert_counts_as_split_inclusive(text: &str) {
    assert_eq!(
        line_count(text),
        text.split_inclusive('\n').count(),
        "{text:?}"
    );
}

#[test]
fn line_count_equals_split_inclusive() {
    for (text, lines) in [
        ("", 0),
        ("no newline", 1),
        ("a\n", 1),
        ("\n", 1),
        ("\n\n", 2),
        ("a\nb", 2),
        ("a\r\nb\r\n", 2),
        ("a\r\nb", 2),
        ("\r", 1),
        ("a\rb", 1),
        ("a\r", 1),
        ("\r\n", 1),
        ("\u{e9}\n\u{1f389}", 2),
    ] {
        assert_eq!(line_count(text), lines, "{text:?}");
        assert_counts_as_split_inclusive(text);
    }
}

/// Every text up to six chars over an alphabet of newlines, a lone return,
/// plain and multi-byte chars.
#[test]
fn line_count_equals_split_inclusive_on_every_short_text() {
    const ALPHABET: [char; 4] = ['\n', '\r', 'a', '\u{e9}'];
    let mut texts = vec![String::new()];
    for _ in 0..6 {
        texts = texts
            .iter()
            .flat_map(|text| {
                assert_counts_as_split_inclusive(text);
                ALPHABET.map(|next| format!("{text}{next}"))
            })
            .collect();
    }
    for text in &texts {
        assert_counts_as_split_inclusive(text);
    }
}
