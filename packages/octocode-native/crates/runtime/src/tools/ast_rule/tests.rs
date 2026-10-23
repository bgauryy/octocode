/// `invalidPattern` is also a rule that does not parse: its recovery
/// names the rule, not only the pattern, within the guidance limit.
#[test]
fn the_invalid_pattern_hint_covers_rules() {
    let hint = super::INVALID_PATTERN_HINT;
    assert!(hint.contains("pattern") && hint.contains("rule"), "{hint}");
    assert!(hint.chars().count() <= crate::response::rows::MAX_GUIDANCE_CHARS);
}

/// A syntax-tree budget stop is terminal: its recovery names no
/// continuation field (syntaxTree pages by `page`/`snapshot`, never an
/// offset) and leads to the tools that reach the unlisted nodes.
#[test]
fn a_tree_budget_recovery_names_no_offset_field() {
    for code in ["syntaxTree.budget.deadline", "syntaxTree.budget.nodeLimit"] {
        let text = super::diagnostic_recovery(code, None).expect("recovery");
        assert!(!text.contains("nextOffset"), "{text}");
        assert!(text.contains("localFetch"), "{text}");
    }
}
