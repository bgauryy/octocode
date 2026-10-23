/// A callee label keeps its receiver chain and drops argument text, so
/// no literal of the source (a planted token) becomes a name.
#[test]
fn callee_labels_elide_arguments_and_literals() {
    for (label, expected) in [
        (
            r#"std::fs::write(&source,"exportconstT=\"ghp_x\";").expect"#,
            "std::fs::write().expect",
        ),
        ("self.config.iter", "self.config.iter"),
        ("items[i+1].map(|x|{x}).len", "items[].map().len"),
        (r#"f(')', "(").g"#, "f().g"),
        (r#""lit".join"#, r#""".join"#),
        ("Vec::<String>::new", "Vec::<String>::new"),
    ] {
        assert_eq!(super::calls::elide_arguments(label), expected, "{label}");
    }
}

#[test]
fn every_grammar_declaration_kind_is_a_declared_declaration_kind() {
    for entry in crate::signatures::languages::all_entries() {
        let language = &entry.language;
        for id in 0..language.node_kind_count() {
            let Some(node_kind) = language.node_kind_for_id(id as u16) else {
                continue;
            };
            if let Some(kind) = super::declarations::declaration_kind(node_kind) {
                assert!(
                    crate::signatures::DECLARATION_KINDS.contains(&kind),
                    "{kind} is missing from DECLARATION_KINDS"
                );
            }
        }
    }
}
