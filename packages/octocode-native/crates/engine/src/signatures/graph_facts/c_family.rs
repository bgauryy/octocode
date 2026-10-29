//! C and C++ declaration facts: a prototype is not its function's
//! definition, and an out-of-line definition names the class or namespace
//! it belongs to (`void Widget::draw() {}`).

use tree_sitter::Node;

/// A function declared without its body: a prototype (`int f(void);`) or
/// an in-class member declaration (`void draw();`).
pub(super) fn is_prototype(node: Node<'_>, kind: &str) -> bool {
    matches!(node.kind(), "declaration" | "field_declaration")
        && matches!(kind, "function" | "method")
}

/// `A::B` of a C++ definition written `void A::B::m() {}`.
#[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
pub(super) fn out_of_line_scope(node: Node<'_>, content: &str) -> Option<String> {
    if node.kind() != "function_definition" {
        return None;
    }
    let mut declarator = node.child_by_field_name("declarator")?;
    while declarator.kind() != "function_declarator" {
        let mut cursor = declarator.walk();
        declarator = declarator.child_by_field_name("declarator").or_else(|| {
            declarator
                .named_children(&mut cursor)
                .find(|child| child.kind().ends_with("_declarator"))
        })?;
    }
    let name = declarator.child_by_field_name("declarator")?;
    if name.kind() != "qualified_identifier" {
        return None;
    }
    crate::signatures::calls::cfamily::written_path(name.child_by_field_name("scope")?, content)
}

#[cfg(not(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda")))]
pub(super) fn out_of_line_scope(_: Node<'_>, _: &str) -> Option<String> {
    None
}
