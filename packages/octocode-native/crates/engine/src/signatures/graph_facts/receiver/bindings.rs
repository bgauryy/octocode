//! Local bindings a receiver resolves through: the per-language binding
//! scans dispatch, binding records and the enclosing-node walk.

use tree_sitter::Node;

use super::*;

impl<'t> ReceiverTypes<'t> {
    pub(super) fn collect_bindings(
        &self,
        up: Up<'_, 't>,
        function: Node<'t>,
        content: &str,
        out: &mut Vec<Binding>,
    ) {
        match self.lang {
            Lang::Rust => self.rust_bindings(up, content, out),
            Lang::Python => self.python_bindings(up, function, content, out),
            Lang::Java => self.java_bindings(up, content, out),
            Lang::Go => self.go_bindings(up, content, out),
            Lang::CSharp => self.csharp_bindings(up, function, content, out),
            Lang::Cpp => self.cpp_bindings(up, content, out),
        }
    }
}

pub(super) fn push(
    out: &mut Vec<Binding>,
    name: Node<'_>,
    ty: Option<String>,
    from: usize,
    until: usize,
    content: &str,
) {
    if let Some(name) = node_text(name, content) {
        out.push(Binding {
            name: name.to_owned(),
            ty,
            from,
            until,
        });
    }
}

/// Bind a plain name to `ty`; every name inside a destructuring pattern is
/// bound with an unknown type so it shadows outer bindings.
pub(super) fn bind_pattern(
    pattern: Node<'_>,
    ty: Option<String>,
    from: usize,
    until: usize,
    content: &str,
    out: &mut Vec<Binding>,
) {
    if pattern.kind() == "identifier" {
        push(out, pattern, ty, from, until, content);
        return;
    }
    let mut pending = vec![pattern];
    let mut cursor = pattern.walk();
    while let Some(node) = pending.pop() {
        if node.kind() == "identifier" {
            push(out, node, None, from, until, content);
        }
        pending.extend(node.named_children(&mut cursor));
    }
}
