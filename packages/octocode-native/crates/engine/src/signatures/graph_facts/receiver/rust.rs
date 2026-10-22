//! Rust receiver bindings.
use super::*;

impl<'t> ReceiverTypes<'t> {
    pub(super) fn rust_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        let pattern = node.child_by_field_name("pattern");
        match node.kind() {
            "parameter" => {
                let Some(pattern) = pattern else { return };
                let until = up.nth(2).unwrap_or(node);
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|ty| self.rust_type(ty, content));
                bind_pattern(pattern, ty, node.end_byte(), until.end_byte(), content, out);
            }
            "closure_parameters" => {
                let until = up.nth(1).unwrap_or(node).end_byte();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() != "parameter" {
                        bind_pattern(child, None, child.end_byte(), until, content, out);
                    }
                }
            }
            "let_declaration" => {
                let Some(pattern) = pattern else { return };
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|ty| self.rust_type(ty, content))
                    .or_else(|| {
                        node.child_by_field_name("value")
                            .and_then(|value| self.rust_constructor(value, content))
                    });
                let until = up.nth(1).unwrap_or(node).end_byte();
                bind_pattern(pattern, ty, node.end_byte(), until, content, out);
            }
            "for_expression" | "match_arm" => {
                let Some(pattern) = pattern else { return };
                bind_pattern(
                    pattern,
                    None,
                    pattern.end_byte(),
                    node.end_byte(),
                    content,
                    out,
                );
            }
            "let_condition" => {
                let Some(pattern) = pattern else { return };
                let until = up
                    .ancestor(&["if_expression", "while_expression"])
                    .unwrap_or(node)
                    .end_byte();
                bind_pattern(pattern, None, node.end_byte(), until, content, out);
            }
            _ => {}
        }
    }

    pub(super) fn rust_type(&self, ty: Node<'t>, content: &str) -> Option<String> {
        let cleaned = clean_type_text(node_text(ty, content)?, Lang::Rust)?;
        self.rust_self(ty, cleaned, content)
    }

    /// `Self` names the enclosing impl's type.
    pub(super) fn rust_self(&self, at: Node<'t>, ty: String, content: &str) -> Option<String> {
        if ty != "Self" {
            return Some(ty);
        }
        let implementation = ancestor(self.root, at, &["impl_item"])?;
        clean_type_text(
            node_text(implementation.child_by_field_name("type")?, content)?,
            Lang::Rust,
        )
    }

    /// `T::new(..)`, `T::from_*(..)`, `T::default()`, `T::with_*(..)` and
    /// `T { .. }`, through `?`, `.await` and `&`.
    pub(super) fn rust_constructor(&self, value: Node<'t>, content: &str) -> Option<String> {
        let mut value = value;
        loop {
            value = match value.kind() {
                "try_expression" | "await_expression" | "parenthesized_expression" => {
                    value.named_child(0)?
                }
                "reference_expression" => value.child_by_field_name("value")?,
                _ => break,
            };
        }
        let ty = match value.kind() {
            "call_expression" => {
                let function = value.child_by_field_name("function")?;
                if function.kind() != "scoped_identifier" {
                    return None;
                }
                let name = node_text(function.child_by_field_name("name")?, content)?;
                let constructor = name == "new"
                    || name == "default"
                    || name.starts_with("new_")
                    || name.starts_with("from")
                    || name.starts_with("with_");
                if !constructor {
                    return None;
                }
                clean_type_text(
                    node_text(function.child_by_field_name("path")?, content)?,
                    Lang::Rust,
                )?
            }
            "struct_expression" => clean_type_text(
                node_text(value.child_by_field_name("name")?, content)?,
                Lang::Rust,
            )?,
            _ => return None,
        };
        starts_uppercase(last_segment(&ty)).then_some(())?;
        self.rust_self(value, ty, content)
    }
}
