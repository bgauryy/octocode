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
