use super::alternation::reads_as_alternation;

/// A `|` splits alternatives only outside escapes and classes, and every
/// alternative must hold text.
#[test]
fn an_alternation_has_text_on_every_side() {
    for text in ["a|b", "(a|b)c", "foo|bar|baz", r"a\.b|c"] {
        assert!(reads_as_alternation(text), "{text}");
    }
    for text in ["a", "a||b", "|a", "a|", r"a\|b", "[a|b]", ""] {
        assert!(!reads_as_alternation(text), "{text}");
    }
}

#[test]
fn invalid_lead_json_exceeds_every_budget() {
    let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
    assert_eq!(
        octocode_engine::json_size::bytes(&invalid).unwrap_or(usize::MAX),
        usize::MAX
    );
}
