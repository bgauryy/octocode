//! Tree-sitter node-kind vocabulary shared across grammars: comments,
//! string literals and annotations. The extractor, the structural matchers
//! and the LSP resolver read these one definitions.

/// A comment node kind (`comment`, `line_comment`, `block_comment`, …).
pub(crate) fn is_comment_kind(kind: &str) -> bool {
    kind.contains("comment")
}

/// A string or character literal node kind (`string`, `string_literal`,
/// `raw_string_literal`, `template_string`, `char_literal`, …).
pub(crate) fn is_string_kind(kind: &str) -> bool {
    kind.contains("string") || kind.contains("char_literal") || kind == "string_content"
}

/// Grammar nodes that annotate the item after them: Rust attributes,
/// decorators, Java and Scala annotations, C# and C++ attribute lists.
pub(crate) fn is_annotation_kind(kind: &str) -> bool {
    matches!(
        kind,
        "attribute_item"
            | "inner_attribute_item"
            | "decorator"
            | "annotation"
            | "marker_annotation"
            | "attribute_list"
            | "attribute_declaration"
    )
}
