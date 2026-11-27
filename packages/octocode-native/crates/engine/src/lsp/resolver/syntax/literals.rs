//! Literal context follows the existing cursor path; interpolation re-enters code.
use crate::signatures::node_kinds::{is_comment_kind, is_string_kind};
use tree_sitter::Node;

pub(super) fn literal_context(parent: bool, node: Node<'_>, field: Option<&str>) -> bool {
    let kind = node.kind();
    if kind.contains("interpolation")
        || kind == "template_substitution"
        || field == Some("interpolator")
    {
        return false;
    }
    // The Scala grammar reads a Scala 2 symbol literal as a quotation of
    // one identifier. A quoted block remains code, not literal text.
    let symbol_quote = kind == "quote_expression"
        && node
            .named_child(0)
            .is_some_and(|child| child.kind() == "identifier");
    parent
        || is_string_kind(kind)
        || is_comment_kind(kind)
        || symbol_quote
        || matches!(
            kind,
            "character_literal"
                | "rune_literal"
                | "symbol_literal"
                | "regex"
                | "regex_pattern"
                | "regex_flags"
                | "jsx_text"
        )
}
