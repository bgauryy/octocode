//! Rust receiver bindings.
use super::*;
use crate::graph::GraphReceiverCall;

/// A Rust function's declared return type and the type `?` yields from it,
/// as receiver types spell them (`Self` is the enclosing impl's type). An
/// `async fn` returns a future, so it has neither. `ancestors` holds the
/// enclosing nodes of `node`, root first.
pub(in crate::signatures::graph_facts) fn rust_return_types(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
) -> (Option<String>, Option<String>) {
    let written = matches!(node.kind(), "function_item" | "function_signature_item")
        .then(|| node.child_by_field_name("return_type"))
        .flatten()
        .and_then(|ty| node_text(ty, content));
    let asynchronous = named_children(node).iter().any(|child| {
        child.kind() == "function_modifiers"
            && node_text(*child, content).is_some_and(|text| text.contains("async"))
    });
    let Some(written) = written.filter(|_| !asynchronous) else {
        return (None, None);
    };
    let named = |text: &str| {
        let ty = clean_type_text(text, Lang::Rust)?;
        if ty != "Self" {
            return Some(ty);
        }
        let implementation = ancestors.iter().rev().find(|n| n.kind() == "impl_item")?;
        clean_type_text(
            node_text(implementation.child_by_field_name("type")?, content)?,
            Lang::Rust,
        )
    };
    (named(written), rust_try_argument(written).and_then(named))
}

/// The call expression a value is, through parentheses, `&` and one `?`
/// (`unwrap`); `.await` changes the type, so it is not followed.
fn value_call(value: Node<'_>) -> Option<(Node<'_>, bool)> {
    let (mut value, mut unwrap) = (value, false);
    loop {
        value = match value.kind() {
            "parenthesized_expression" => value.named_child(0)?,
            "reference_expression" => value.child_by_field_name("value")?,
            "try_expression" if !unwrap => {
                unwrap = true;
                value.named_child(0)?
            }
            _ => break,
        };
    }
    Some((value, unwrap))
}

fn call_reference(call: Node<'_>, unwrap: bool) -> GraphReceiverCall {
    let byte = |offset: usize| u32::try_from(offset).unwrap_or(u32::MAX);
    GraphReceiverCall {
        start_byte: byte(call.start_byte()),
        end_byte: byte(call.end_byte()),
        unwrap,
    }
}

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
                let value_call = node
                    .child_by_field_name("value")
                    .and_then(value_call)
                    .filter(|(value, _)| ty.is_none() && value.kind() == "call_expression")
                    .map(|(value, unwrap)| call_reference(value, unwrap));
                bind_pattern(pattern, ty, node.end_byte(), until, content, out);
                if pattern.kind() == "identifier"
                    && let Some(binding) = out.last_mut()
                {
                    binding.value_call = value_call;
                }
            }
            // An item in a block is visible in the whole block.
            "static_item" | "const_item" => {
                let (Some(name), Some(block)) = (node.child_by_field_name("name"), up.nth(1))
                else {
                    return;
                };
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|ty| self.rust_type(ty, content));
                push(out, name, ty, block.start_byte(), block.end_byte(), content);
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

    /// The declared type of the file-level `static` or `const` named `name`.
    pub(super) fn rust_item_type(&self, name: &str, content: &str) -> Option<String> {
        let item = named_children(self.root).into_iter().find(|item| {
            matches!(item.kind(), "static_item" | "const_item")
                && item
                    .child_by_field_name("name")
                    .and_then(|n| node_text(n, content))
                    == Some(name)
        })?;
        self.rust_type(item.child_by_field_name("type")?, content)
    }

    /// The call whose value the receiver of the member call `call` is: the
    /// receiver itself (`T::open()?.m()`) or the call an untyped `let` local
    /// was bound to. `path` holds the enclosing nodes of `call`, root first.
    /// `typed` says the receiver already has a type: a typed local never
    /// keeps its value call, so its binding is not looked up again.
    pub(in crate::signatures::graph_facts) fn rust_receiver_call(
        &mut self,
        call: Node<'t>,
        path: &[Node<'t>],
        typed: bool,
        content: &str,
    ) -> Option<GraphReceiverCall> {
        let function = call.child_by_field_name("function")?;
        let function = if function.kind() == "generic_function" {
            function.child_by_field_name("function")?
        } else {
            function
        };
        let (receiver, unwrap) = value_call(self.lang.member_parts(function)?.0)?;
        match receiver.kind() {
            "call_expression" => Some(call_reference(receiver, unwrap)),
            "identifier" if !typed => {
                let name = node_text(receiver, content)?;
                let binding = self.local(receiver, path, name, content)?;
                let mut value = binding.value_call.filter(|_| binding.ty.is_none())?;
                if unwrap {
                    (!value.unwrap).then_some(())?;
                    value.unwrap = true;
                }
                Some(value)
            }
            _ => None,
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

    /// `T::new(..)`, `T::from_*(..)`, `T::default()`, `T::with_*(..)`,
    /// `T { .. }` and a unit struct `T`, through `?`, `.await` and `&`.
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
            // A unit struct value (`Square`): CamelCase, unlike a constant.
            "identifier" => node_text(value, content)
                .filter(|name| name.chars().any(char::is_lowercase))?
                .to_owned(),
            _ => return None,
        };
        starts_uppercase(last_segment(&ty)).then_some(())?;
        self.rust_self(value, ty, content)
    }
}
