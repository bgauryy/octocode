//! Bounding each pattern's lazy-DFA cache ([`PATTERN_CACHE_BYTES`]) never
//! changes what a pattern matches: an engine that outgrows its cache falls
//! back to a slower one with the same semantics.

use super::*;

/// A haystack of secrets among varied non-ASCII and ASCII text, large enough
/// that a lazy DFA over it needs more states than the bound allows.
fn haystack() -> String {
    let mut text = String::new();
    let mut state = 0x9E37_79B9_u32;
    let mut word = |len: usize| -> String {
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pick = (state >> 24) as usize;
                [
                    'a', 'Z', '0', '9', '_', '-', '=', ':', '/', '+', ' ', '\n', 'é', 'ß', '日',
                    '🔑',
                ][pick % 16]
            })
            .collect()
    };
    for round in 0..4_000 {
        text.push_str(&word(40 + round % 50));
        if round % 400 == 0 {
            text.push_str(&format!(
                "\nAPI_TOKEN = \"{FAKE_GH_TOKEN}\"\naws_key: {FAKE_AWS_KEY}\n"
            ));
            text.push_str(&rsa_key(3));
            text.push('\n');
        }
    }
    text
}

fn spans(regex: &Regex, text: &str) -> Vec<(usize, usize)> {
    regex.find_iter(text).map(|m| (m.start(), m.end())).collect()
}

#[test]
fn a_bounded_cache_finds_what_an_unbounded_one_finds() {
    let text = haystack();
    assert!(text.len() > 4 * PATTERN_CACHE_BYTES, "{}", text.len());
    let mut found = 0;
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        let source = ascii_word_boundaries(pattern.regex);
        let unbounded = Regex::new(&source).expect(pattern.name);
        let bounded = pattern_regex(idx);
        let expected = spans(&unbounded, &text);
        found += expected.len();
        assert_eq!(spans(bounded, &text), expected, "pattern {}", pattern.name);
    }
    assert!(found > 0, "the haystack holds secrets no pattern found");
}

#[test]
fn redaction_with_a_bounded_cache_is_stable_on_a_large_secret_fixture() {
    let text = haystack();
    let once = detect(&text, None);
    assert!(once.sanitized.contains("[REDACTED-"));
    assert!(!once.secrets_detected.is_empty());
    assert_no_pattern_matches(&once);
    assert!(!once.sanitized.contains(FAKE_GH_TOKEN));
    assert!(!once.sanitized.contains(FAKE_AWS_KEY));
    // Deterministic: a second pass over the same text redacts the same.
    let again = detect(&text, None);
    assert_eq!(once.sanitized, again.sanitized);
    assert_eq!(once.secrets_detected, again.secrets_detected);
}
