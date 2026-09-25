//! Grammar node-kind lookup shared by the structural matchers.

use tree_sitter::Language;

/// Symbol id of a user-supplied **named** node kind, or `None` when no parsed
/// node can carry it.
///
/// `Language::id_for_node_kind` is a single table lookup, but it maps
/// supertypes (e.g. `expression`) to ids no concrete node carries and any
/// prefix of `ERROR` (including `""`) to the ERROR symbol. Both are rejected
/// here so a rule naming them fails loudly instead of silently matching
/// nothing or the wrong nodes. `ERROR` itself (tree-sitter's recovery node,
/// outside the grammar's symbol table) is accepted.
pub(super) fn named_kind_id(language: &Language, kind: &str) -> Option<u16> {
    if kind == "ERROR" {
        return Some(u16::MAX);
    }
    let id = language.id_for_node_kind(kind, true);
    if id == 0 || id == u16::MAX || language.node_kind_is_supertype(id) {
        None
    } else {
        Some(id)
    }
}

#[cfg(test)]
mod tests {
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
}
