//! Go receiver bindings.
use super::*;

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
                bind_pattern(*name, ty, node.end_byte(), until, content, out);
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
                push(out, *name, ty, node.end_byte(), until, content);
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
