//! The C++/CUDA callee label: the names a call is looked up by, without the
//! template arguments that only select a specialization (`f<A, B>(x)` calls
//! `f`), and with `->` member access written as `.` like every other member
//! call (`p->run()` and `(*p).run()` are `p.run`).

use tree_sitter::Node;

use super::node_text;

/// Deeper callee nesting than any written name path.
const MAX_DEPTH: u8 = 16;

/// `(label, name token)` of the `function` of a C++/CUDA `call_expression`,
/// or `None` when the callee is no name path (a parenthesized or indexed
/// expression), which the ordinary label then spells.
pub(super) fn callee<'tree>(function: Node<'tree>, content: &str) -> Option<(String, Node<'tree>)> {
    label(function, content, 0)
}

/// The name path `node` spells (`util::Widget` for `util::Widget<T>`), or
/// `None` when it is no name path.
pub(in crate::signatures) fn written_path(node: Node<'_>, content: &str) -> Option<String> {
    label(node, content, 0).map(|(path, _)| path)
}

fn label<'tree>(node: Node<'tree>, content: &str, depth: u8) -> Option<(String, Node<'tree>)> {
    if depth >= MAX_DEPTH || node.is_missing() || node.has_error() {
        return None;
    }
    match node.kind() {
        "identifier" | "field_identifier" | "namespace_identifier" | "type_identifier" | "this" => {
            node_text(node, content)
                .filter(|text| !text.is_empty())
                .map(|text| (text.to_owned(), node))
        }
        "template_function" | "template_method" | "template_type" => {
            label(node.child_by_field_name("name")?, content, depth + 1)
        }
        "qualified_identifier" => {
            let (name, token) = label(node.child_by_field_name("name")?, content, depth + 1)?;
            match node.child_by_field_name("scope") {
                Some(scope) => {
                    let (scope, _) = label(scope, content, depth + 1)?;
                    Some((format!("{scope}::{name}"), token))
                }
                None => Some((format!("::{name}"), token)),
            }
        }
        "field_expression" => {
            let field = node.child_by_field_name("field")?;
            let (name, token) = label(field, content, depth + 1)?;
            // `p->B::x()` names `B`'s member, as `B::x()` does.
            if field.kind() == "qualified_identifier" {
                return Some((name, token));
            }
            let receiver = dereferenced(node.child_by_field_name("argument")?, content);
            let (receiver, _) = label(receiver, content, depth + 1)?;
            Some((format!("{receiver}.{name}"), token))
        }
        _ => None,
    }
}

/// The object a member access reads: `(*p).x` reads `p`, as `p->x` does.
pub(in crate::signatures) fn dereferenced<'tree>(
    mut node: Node<'tree>,
    content: &str,
) -> Node<'tree> {
    loop {
        let inner = match node.kind() {
            "parenthesized_expression" => node.named_child(0),
            "pointer_expression"
                if node
                    .child_by_field_name("operator")
                    .is_some_and(|operator| node_text(operator, content) == Some("*")) =>
            {
                node.child_by_field_name("argument")
            }
            _ => None,
        };
        match inner {
            Some(inner) => node = inner,
            None => return node,
        }
    }
}
