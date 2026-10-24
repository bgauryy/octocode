//! Java receiver bindings.
use super::*;

pub(super) fn java_bindings<'t>(up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
    let node = up.node;
    let declared = |ty: Option<Node<'t>>, value: Option<Node<'t>>| {
        let ty = ty?;
        if node_text(ty, content) == Some("var") {
            value.and_then(|value| java_constructor(value, content))
        } else {
            clean_type_text(node_text(ty, content)?, Lang::Java)
        }
    };
    match node.kind() {
        "formal_parameter" | "catch_formal_parameter" => {
            let Some(name) = node.child_by_field_name("name") else {
                return;
            };
            let until = up.nth(2).unwrap_or(node);
            let ty = if node.kind() == "catch_formal_parameter" {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .find(|child| child.kind() == "catch_type")
                    .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Java))
            } else {
                declared(node.child_by_field_name("type"), None)
            };
            push(out, name, ty, node.end_byte(), until.end_byte(), content);
        }
        "spread_parameter" => {
            let until = up.nth(2).unwrap_or(node);
            bind_pattern(node, None, node.end_byte(), until.end_byte(), content, out);
        }
        "local_variable_declaration" => {
            let ty = node.child_by_field_name("type");
            let until = up.nth(1).unwrap_or(node).end_byte();
            let mut cursor = node.walk();
            for declarator in node.children_by_field_name("declarator", &mut cursor) {
                let Some(name) = declarator.child_by_field_name("name") else {
                    continue;
                };
                let value = declarator.child_by_field_name("value");
                push(
                    out,
                    name,
                    declared(ty, value),
                    declarator.end_byte(),
                    until,
                    content,
                );
            }
        }
        "enhanced_for_statement" | "resource" => {
            let Some(name) = node.child_by_field_name("name") else {
                return;
            };
            let ty = declared(
                node.child_by_field_name("type"),
                node.child_by_field_name("value"),
            );
            let until = if node.kind() == "resource" {
                up.ancestor(&["try_with_resources_statement"])
                    .unwrap_or(node)
            } else {
                node
            };
            push(out, name, ty, name.end_byte(), until.end_byte(), content);
        }
        "lambda_expression" => {
            if let Some(parameters) = node.child_by_field_name("parameters")
                && parameters.kind() != "formal_parameters"
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

pub(super) fn java_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "object_creation_expression").then_some(())?;
    clean_type_text(
        node_text(value.child_by_field_name("type")?, content)?,
        Lang::Java,
    )
}
