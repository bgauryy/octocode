//! Rust receiver bindings.
use super::*;
use crate::graph::GraphReceiverCall;
use crate::num::saturating_u32;
use crate::signatures::node_kinds::kind_of;

/// A function's declared return types, as receiver types spell them
/// (`Self` is the enclosing impl's type): the type, the type `?` yields from
/// it, each element of a tuple, and whether an `async fn` yields them from
/// its future.
#[derive(Default)]
pub(in crate::signatures::graph_facts) struct DeclaredReturns {
    pub(in crate::signatures::graph_facts) ty: Option<String>,
    pub(in crate::signatures::graph_facts) try_ty: Option<String>,
    pub(in crate::signatures::graph_facts) elements: Vec<Option<String>>,
    pub(in crate::signatures::graph_facts) asynchronous: bool,
}

/// [`DeclaredReturns`] of a function item. `ancestors` holds the enclosing nodes
/// of `node`, root first.
pub(in crate::signatures::graph_facts) fn rust_return_types(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
) -> DeclaredReturns {
    let written_node = matches!(kind_of(node), "function_item" | "function_signature_item")
        .then(|| node.child_by_field_name("return_type"))
        .flatten();
    let written = written_node.and_then(|ty| node_text(ty, content));
    let asynchronous = named_children(node).iter().any(|child| {
        kind_of(child) == "function_modifiers"
            && node_text(*child, content).is_some_and(|text| text.contains("async"))
    });
    let Some(written) = written else {
        return DeclaredReturns::default();
    };
    let named = |text: &str| {
        let ty = clean_type_text(text, Lang::Rust)?;
        if ty != "Self" {
            return Some(ty);
        }
        let implementation = ancestors
            .iter()
            .rev()
            .find(|n| kind_of(*n) == "impl_item")?;
        clean_type_text(
            node_text(implementation.child_by_field_name("type")?, content)?,
            Lang::Rust,
        )
    };
    let elements = written_node
        .filter(|ty| kind_of(*ty) == "tuple_type")
        .map(|ty| {
            named_children(ty)
                .iter()
                .map(|element| node_text(*element, content).and_then(named))
                .collect()
        })
        .unwrap_or_default();
    DeclaredReturns {
        ty: named(written),
        try_ty: rust_try_argument(written).and_then(named),
        elements,
        asynchronous,
    }
}

/// The call expression a value is, through parentheses, `&`, one `?`
/// (`unwrap`) and one `.await` (`awaited`: the value is what the call's
/// future yields).
fn value_call(value: Node<'_>) -> Option<(Node<'_>, bool, bool)> {
    let (mut value, mut unwrap, mut awaited) = (value, false, false);
    loop {
        value = match kind_of(value) {
            "parenthesized_expression" => value.named_child(0)?,
            "reference_expression" => value.child_by_field_name("value")?,
            "try_expression" if !unwrap => {
                unwrap = true;
                value.named_child(0)?
            }
            "await_expression" if !awaited => {
                awaited = true;
                value.named_child(0)?
            }
            _ => break,
        };
    }
    Some((value, unwrap, awaited))
}

pub(super) fn call_reference(call: Node<'_>, unwrap: bool, awaited: bool) -> GraphReceiverCall {
    let byte = |offset: usize| saturating_u32(offset);
    GraphReceiverCall {
        start_byte: byte(call.start_byte()),
        end_byte: byte(call.end_byte()),
        unwrap,
        awaited,
        element: None,
    }
}

/// The names a tuple pattern binds by element, up to a `..` rest (whose
/// later elements count from the end): `(a, mut b, _, ..)` gives `a` at 0
/// and `b` at 1.
fn tuple_elements<'a>(pattern: Node<'_>, content: &'a str) -> Vec<(u32, &'a str)> {
    let mut named = Vec::new();
    for (index, element) in named_children(pattern).into_iter().enumerate() {
        let element = match kind_of(element) {
            "mut_pattern" | "ref_pattern" => match element.named_child(0) {
                Some(inner) => inner,
                None => continue,
            },
            "remaining_field_pattern" => break,
            _ => element,
        };
        if kind_of(element) == "identifier"
            && let Some(name) = node_text(element, content)
        {
            named.push((saturating_u32(index), name));
        }
    }
    named
}

impl<'t> ReceiverTypes<'t> {
    pub(super) fn rust_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        let pattern = node.child_by_field_name("pattern");
        match kind_of(node) {
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
                    if kind_of(child) != "parameter" {
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
                    .filter(|(value, ..)| ty.is_none() && kind_of(value) == "call_expression")
                    .map(|(value, unwrap, awaited)| call_reference(value, unwrap, awaited));
                let first = out.len();
                bind_pattern(pattern, ty, node.end_byte(), until, content, out);
                if kind_of(pattern) == "identifier"
                    && let Some(binding) = out.last_mut()
                {
                    binding.value_call = value_call;
                } else if kind_of(pattern) == "tuple_pattern"
                    && let Some(call) = value_call
                {
                    // Each name a tuple pattern binds takes its element of the value.
                    for (element, name) in tuple_elements(pattern, content) {
                        if let Some(binding) = out[first..].iter_mut().find(|b| b.name == name) {
                            binding.value_call = Some(GraphReceiverCall {
                                element: Some(element),
                                ..call
                            });
                        }
                    }
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
            matches!(kind_of(item), "static_item" | "const_item")
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
        let function = if kind_of(function) == "generic_function" {
            function.child_by_field_name("function")?
        } else {
            function
        };
        let (receiver, unwrap, awaited) = value_call(self.lang.member_parts(function)?.0)?;
        match kind_of(receiver) {
            "call_expression" => Some(call_reference(receiver, unwrap, awaited)),
            "identifier" if !typed => {
                let name = node_text(receiver, content)?;
                let binding = self.local(receiver, path, name, content)?;
                let mut value = binding.value_call.filter(|_| binding.ty.is_none())?;
                // `x?` and `x.await` on a local bound to a call apply once, to
                // the call's whole value (not to one tuple element).
                if unwrap {
                    (!value.unwrap).then_some(())?;
                    value.unwrap = true;
                }
                if awaited {
                    (!value.awaited && value.element.is_none()).then_some(())?;
                    value.awaited = true;
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
            value = match kind_of(value) {
                "try_expression" | "await_expression" | "parenthesized_expression" => {
                    value.named_child(0)?
                }
                "reference_expression" => value.child_by_field_name("value")?,
                _ => break,
            };
        }
        let ty = match kind_of(value) {
            "call_expression" => {
                let function = value.child_by_field_name("function")?;
                if kind_of(function) != "scoped_identifier" {
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
