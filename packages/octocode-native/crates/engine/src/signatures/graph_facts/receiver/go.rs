//! Go receiver bindings.
use super::rust::call_reference;
use super::*;
use crate::graph::GraphReceiverCall;

/// The type of a Go function's or method's first result, as written in its
/// package (`*Store` is `Store`, `pkg.T` stays `pkg.T`): the value a
/// `x, err := f()` binds to `x`. A result that names no type (slice, map,
/// function), a result named by a type parameter (`func F[T any]() T`: the
/// caller's type argument, not a type of the package) or a declaration
/// without results has none.
pub(in crate::signatures::graph_facts) fn go_return_type(
    node: Node<'_>,
    content: &str,
) -> Option<String> {
    matches!(node.kind(), "function_declaration" | "method_declaration").then_some(())?;
    let result = node.child_by_field_name("result")?;
    let first = if result.kind() == "parameter_list" {
        named_children(result)
            .into_iter()
            .next()?
            .child_by_field_name("type")?
    } else {
        result
    };
    let ty = clean_type_text(node_text(first, content)?, Lang::Go)?;
    (!type_parameters(node, content).contains(&ty.as_str())).then_some(ty)
}

/// The names a Go declaration binds as type parameters: its own `[T any]`
/// list and, for a method, the type arguments of its receiver
/// (`func (b Box[T]) Get() T`).
fn type_parameters<'a>(node: Node<'_>, content: &'a str) -> Vec<&'a str> {
    let mut names = Vec::new();
    if let Some(list) = node.child_by_field_name("type_parameters") {
        for declaration in named_children(list) {
            let mut cursor = declaration.walk();
            names.extend(
                declaration
                    .children_by_field_name("name", &mut cursor)
                    .filter_map(|name| node_text(name, content)),
            );
        }
    }
    if let Some(receiver) = node.child_by_field_name("receiver") {
        receiver_arguments(receiver, false, content, &mut names);
    }
    names
}

/// The type identifiers written inside a receiver's type arguments.
fn receiver_arguments<'a>(
    node: Node<'_>,
    in_arguments: bool,
    content: &'a str,
    names: &mut Vec<&'a str>,
) {
    if in_arguments && node.kind() == "type_identifier" {
        names.extend(node_text(node, content));
    }
    let in_arguments = in_arguments || node.kind() == "type_arguments";
    for child in named_children(node) {
        receiver_arguments(child, in_arguments, content, names);
    }
}

/// The call expression a Go value is, through parentheses.
fn value_call(mut value: Node<'_>) -> Option<Node<'_>> {
    while value.kind() == "parenthesized_expression" {
        value = value.named_child(0)?;
    }
    (value.kind() == "call_expression").then_some(value)
}

/// The call that supplies the value of binding `index` of a Go assignment of
/// `rights` to `lefts`: a call per name, or the first name of a multi-value
/// call (`s, err := f()`), whose first result it is.
fn bound_call<'t>(index: usize, names: usize, rights: &[Node<'t>]) -> Option<Node<'t>> {
    let value = match (names == rights.len(), index, rights) {
        (true, _, _) => rights.get(index)?,
        (false, 0, [only]) => only,
        _ => return None,
    };
    value_call(*value)
}

impl<'t> ReceiverTypes<'t> {
    /// The call whose value the receiver of the Go member call `call` is: the
    /// receiver itself (`f().M()`, `pkg.F().M()`) or the call a local
    /// without a written type was bound to. The receiver's `receiver_type`
    /// beside it is only a constructor-name guess (`NewT()` is a `T`), which
    /// the callee's declared result outranks.
    pub(in crate::signatures::graph_facts) fn go_receiver_call(
        &mut self,
        call: Node<'t>,
        path: &[Node<'t>],
        content: &str,
    ) -> Option<GraphReceiverCall> {
        let function = call.child_by_field_name("function")?;
        let function = if function.kind() == "generic_function" {
            function.child_by_field_name("function")?
        } else {
            function
        };
        let mut receiver = self.lang.member_parts(function)?.0;
        while receiver.kind() == "parenthesized_expression" {
            receiver = receiver.named_child(0)?;
        }
        match receiver.kind() {
            "call_expression" => Some(call_reference(receiver, false)),
            "identifier" => {
                let name = node_text(receiver, content)?;
                self.local(receiver, path, name, content)?.value_call
            }
            _ => None,
        }
    }
}

pub(super) fn go_bindings<'t>(up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
    let node = up.node;
    match node.kind() {
        "parameter_declaration" | "variadic_parameter_declaration" => {
            let Some(owner) = up.nth(2) else {
                return;
            };
            if !matches!(
                owner.kind(),
                "function_declaration" | "method_declaration" | "func_literal"
            ) {
                return;
            }
            let ty = (node.kind() == "parameter_declaration")
                .then(|| node.child_by_field_name("type"))
                .flatten()
                .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Go));
            let mut cursor = node.walk();
            for name in node.children_by_field_name("name", &mut cursor) {
                push(
                    out,
                    name,
                    ty.clone(),
                    node.end_byte(),
                    owner.end_byte(),
                    content,
                );
            }
        }
        "short_var_declaration" => {
            let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) else {
                return;
            };
            let until = go_scope_end(up);
            let lefts = named_children(left);
            let rights = named_children(right);
            for (index, name) in lefts.iter().enumerate() {
                let ty = if lefts.len() == rights.len() {
                    go_constructor(rights[index], false, content)
                } else if index == 0 && rights.len() == 1 {
                    go_constructor(rights[0], true, content)
                } else {
                    None
                };
                let value = bound_call(index, lefts.len(), &rights)
                    .filter(|_| name.kind() == "identifier")
                    .map(|call| call_reference(call, false));
                bind_pattern(*name, ty, node.end_byte(), until, content, out);
                if let Some(binding) = out.last_mut().filter(|_| value.is_some()) {
                    binding.value_call = value;
                }
            }
        }
        "var_spec" => {
            let until = go_scope_end(up);
            let declared = node
                .child_by_field_name("type")
                .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Go));
            let values = node
                .child_by_field_name("value")
                .map(named_children)
                .unwrap_or_default();
            let mut cursor = node.walk();
            let names: Vec<_> = node.children_by_field_name("name", &mut cursor).collect();
            for (index, name) in names.iter().enumerate() {
                let ty = declared.clone().or_else(|| {
                    if values.len() == names.len() {
                        go_constructor(values[index], false, content)
                    } else if index == 0 && values.len() == 1 {
                        go_constructor(values[0], true, content)
                    } else {
                        None
                    }
                });
                let value = bound_call(index, names.len(), &values)
                    .filter(|_| declared.is_none())
                    .map(|call| call_reference(call, false));
                push(out, *name, ty, node.end_byte(), until, content);
                if let Some(binding) = out.last_mut().filter(|_| value.is_some()) {
                    binding.value_call = value;
                }
            }
        }
        "range_clause" => {
            if let Some(left) = node.child_by_field_name("left") {
                let until = up.nth(1).unwrap_or(node).end_byte();
                bind_pattern(left, None, node.end_byte(), until, content, out);
            }
        }
        "type_switch_statement" => {
            if let Some(alias) = node.child_by_field_name("alias") {
                bind_pattern(alias, None, alias.end_byte(), node.end_byte(), content, out);
            }
        }
        _ => {}
    }
}

/// The end of the block that holds a Go declaration statement.
pub(super) fn go_scope_end(up: Up<'_, '_>) -> usize {
    let mut level = 1;
    while let Some(parent) = up.nth(level) {
        match parent.kind() {
            "var_declaration" | "var_spec_list" => level += 1,
            "statement_list" => return up.nth(level + 1).unwrap_or(parent).end_byte(),
            _ => return parent.end_byte(),
        }
    }
    up.node.end_byte()
}

/// `T{..}`, `&T{..}`, `new(T)`, `NewT(..)` → `T`, `pkg.NewT(..)` → `pkg.T`.
/// A multi-value assignment (`x, err := …`) only reads constructor calls.
pub(super) fn go_constructor(value: Node<'_>, multi: bool, content: &str) -> Option<String> {
    let literal = |node: Node<'_>| {
        (node.kind() == "composite_literal")
            .then(|| {
                clean_type_text(
                    node_text(node.child_by_field_name("type")?, content)?,
                    Lang::Go,
                )
            })
            .flatten()
    };
    match value.kind() {
        "composite_literal" if !multi => literal(value),
        "unary_expression" if !multi => {
            let operator = value.child_by_field_name("operator")?;
            (node_text(operator, content) == Some("&")).then_some(())?;
            literal(value.child_by_field_name("operand")?)
        }
        "call_expression" => {
            let function = value.child_by_field_name("function")?;
            let (package, name) = match function.kind() {
                "identifier" => (None, node_text(function, content)?),
                "selector_expression" => {
                    let package = function.child_by_field_name("operand")?;
                    (package.kind() == "identifier").then_some(())?;
                    (
                        Some(node_text(package, content)?),
                        node_text(function.child_by_field_name("field")?, content)?,
                    )
                }
                _ => return None,
            };
            if package.is_none() && name == "new" && !multi {
                let argument = value.child_by_field_name("arguments")?.named_child(0)?;
                return clean_type_text(node_text(argument, content)?, Lang::Go);
            }
            let ty = name
                .strip_prefix("New")
                .filter(|rest| starts_uppercase(rest))?;
            Some(match package {
                Some(package) => format!("{package}.{ty}"),
                None => ty.to_owned(),
            })
        }
        _ => None,
    }
}
