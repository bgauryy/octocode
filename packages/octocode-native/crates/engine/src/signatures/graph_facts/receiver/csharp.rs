//! C# receiver bindings.
use super::*;

impl<'t> ReceiverTypes<'t> {
    pub(super) fn csharp_bindings(
        &self,
        up: Up<'_, 't>,
        function: Node<'t>,
        content: &str,
        out: &mut Vec<Binding>,
    ) {
        let node = up.node;
        let declared = |ty: Option<Node<'t>>| {
            let ty = ty?;
            (ty.kind() != "implicit_type")
                .then(|| clean_type_text(node_text(ty, content)?, Lang::CSharp))
                .flatten()
        };
        match node.kind() {
            "parameter" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let until = up.nth(2).unwrap_or(node);
                let ty = declared(node.child_by_field_name("type"));
                push(out, name, ty, node.end_byte(), until.end_byte(), content);
            }
            "variable_declaration" => {
                let Some(parent) = up.nth(1) else { return };
                if matches!(
                    parent.kind(),
                    "field_declaration" | "event_field_declaration"
                ) {
                    return;
                }
                let scope = if parent.kind() == "local_declaration_statement" {
                    up.nth(2).unwrap_or(parent)
                } else {
                    parent
                };
                let ty = node.child_by_field_name("type");
                let mut cursor = node.walk();
                for declarator in node.named_children(&mut cursor) {
                    if declarator.kind() != "variable_declarator" {
                        continue;
                    }
                    let Some(name) = declarator.child_by_field_name("name") else {
                        continue;
                    };
                    let resolved = if ty.is_some_and(|ty| ty.kind() == "implicit_type") {
                        csharp_initializer(declarator, name)
                            .and_then(|value| csharp_constructor(value, content))
                    } else {
                        declared(ty)
                    };
                    push(
                        out,
                        name,
                        resolved,
                        declarator.end_byte(),
                        scope.end_byte(),
                        content,
                    );
                }
            }
            "foreach_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let ty = declared(node.child_by_field_name("type"));
                    bind_pattern(left, ty, left.end_byte(), node.end_byte(), content, out);
                }
            }
            "declaration_pattern" | "declaration_expression" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let ty = declared(node.child_by_field_name("type"));
                    let until = up.ancestor(&[function.kind()]).unwrap_or(function);
                    bind_pattern(name, ty, node.end_byte(), until.end_byte(), content, out);
                }
            }
            "catch_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let ty = declared(node.child_by_field_name("type"));
                    let until = up.nth(1).unwrap_or(node).end_byte();
                    push(out, name, ty, node.end_byte(), until, content);
                }
            }
            "lambda_expression" => {
                if let Some(parameters) = node.child_by_field_name("parameters")
                    && parameters.kind() != "parameter_list"
                {
                    bind_pattern(
                        parameters,
                        None,
                        parameters.end_byte(),
                        node.end_byte(),
                        content,
                        out,
                    );
                }
            }
            _ => {}
        }
    }
}

/// The initializer of a C# `variable_declarator` (`x = value`).
pub(super) fn csharp_initializer<'t>(declarator: Node<'t>, name: Node<'t>) -> Option<Node<'t>> {
    let value = named_children(declarator)
        .into_iter()
        .rfind(|child| *child != name && child.kind() != "bracketed_argument_list")?;
    if value.kind() == "equals_value_clause" {
        value.named_child(0)
    } else {
        Some(value)
    }
}

pub(super) fn csharp_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "object_creation_expression").then_some(())?;
    clean_type_text(
        node_text(value.child_by_field_name("type")?, content)?,
        Lang::CSharp,
    )
}
