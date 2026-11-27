//! C and C++ declaration facts: a prototype is not its function's
//! definition, and an out-of-line definition names the class or namespace
//! it belongs to (`void Widget::draw() {}`).

use crate::signatures::node_kinds::kind_of;
use tree_sitter::Node;

/// Drops from `children`, the children of `node`, the branch of a
/// preprocessor conditional whose condition is a literal: `#if 0` keeps only
/// its `#else`/`#elif` chain, `#if 1` loses it. Any other condition needs the
/// build to decide, so both branches stay candidates.
pub(in crate::signatures) fn retain_compiled<'t>(
    node: Node<'t>,
    content: &str,
    children: &mut Vec<Node<'t>>,
) {
    if !matches!(kind_of(node), "preproc_if" | "preproc_elif") {
        return;
    }
    let Some(compiled) = node
        .child_by_field_name("condition")
        .and_then(|condition| literal_truth(condition, content))
    else {
        return;
    };
    let alternative = node.child_by_field_name("alternative");
    children.retain(|child| alternative.is_some_and(|other| other.id() == child.id()) != compiled);
}

/// The truth of an integer literal: zero in any base and with any suffix is
/// false.
fn literal_truth(condition: Node<'_>, content: &str) -> Option<bool> {
    if kind_of(condition) != "number_literal" {
        return None;
    }
    let text = crate::signatures::nodes::node_text(condition, content)?;
    let lower = text.to_ascii_lowercase();
    let unsuffixed = lower.trim_end_matches(['u', 'l']);
    let digits = unsuffixed
        .strip_prefix("0x")
        .or_else(|| unsuffixed.strip_prefix("0b"))
        .unwrap_or(unsuffixed);
    Some(digits.chars().any(|digit| digit != '0' && digit != '\''))
}

/// A function declared without its body: a prototype (`int f(void);`) or
/// an in-class member declaration (`void draw();`).
pub(super) fn is_prototype(node: Node<'_>, kind: &str) -> bool {
    matches!(kind_of(node), "declaration" | "field_declaration")
        && matches!(kind, "function" | "method")
}

/// `A::B` of a C++ definition written `void A::B::m() {}`.
#[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
pub(super) fn out_of_line_scope(node: Node<'_>, content: &str) -> Option<String> {
    if kind_of(node) != "function_definition" {
        return None;
    }
    let mut declarator = node.child_by_field_name("declarator")?;
    while kind_of(declarator) != "function_declarator" {
        let mut cursor = declarator.walk();
        declarator = declarator.child_by_field_name("declarator").or_else(|| {
            declarator
                .named_children(&mut cursor)
                .find(|child| kind_of(child).ends_with("_declarator"))
        })?;
    }
    let name = declarator.child_by_field_name("declarator")?;
    if kind_of(name) != "qualified_identifier" {
        return None;
    }
    crate::signatures::calls::cfamily::written_path(name.child_by_field_name("scope")?, content)
}

#[cfg(not(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda")))]
pub(super) fn out_of_line_scope(_: Node<'_>, _: &str) -> Option<String> {
    None
}
