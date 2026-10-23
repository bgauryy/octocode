use super::*;

#[test]
fn rewrites_only_word_boundary_escapes() {
    assert_eq!(
        ascii_word_boundaries(r"\bsk-\d+\b"),
        r"(?-u:\b)sk-\d+(?-u:\b)"
    );
    assert_eq!(ascii_word_boundaries(r"a\\b"), r"a\\b");
}

#[test]
fn every_pattern_compiles_and_still_matches_next_to_non_ascii_text() {
    for idx in 0..PATTERNS.len() {
        let _ = pattern_regex(idx);
    }
    let idx = PATTERNS
        .iter()
        .position(|p| p.regex.starts_with(r"\bsk-proj-"))
        .expect("openai project key pattern");
    let text = "é sk-proj-abcdefghijklmnopqrstuvwx ü";
    assert!(pattern_regex(idx).is_match(text));
}
