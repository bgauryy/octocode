use super::named_kind_id;

#[test]
fn rejects_supertypes_error_prefixes_and_unknown_kinds() {
    let ts: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    assert!(named_kind_id(&ts, "call_expression").is_some());
    assert_eq!(named_kind_id(&ts, "ERROR"), Some(u16::MAX));
    for rejected in ["", "E", "ERR", "expression", "statement", "not_a_kind", "("] {
        assert_eq!(named_kind_id(&ts, rejected), None, "{rejected:?}");
    }
}
