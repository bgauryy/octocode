//! C and C++ receiver bindings.
use super::*;

impl<'t> ReceiverTypes<'t> {
    pub(super) fn cpp_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        let declared = |ty: Option<Node<'t>>| {
            let ty = ty?;
            (ty.kind() != "placeholder_type_specifier")
                .then(|| clean_type_text(node_text(ty, content)?, Lang::Cpp))
                .flatten()
        };
        match node.kind() {
            "parameter_declaration" | "optional_parameter_declaration" => {
                let Some(name) = node
                    .child_by_field_name("declarator")
                    .and_then(cpp_declarator_name)
                else {
                    return;
                };
                let until = up
                    .ancestor(&["function_definition", "lambda_expression"])
                    .unwrap_or(node)
                    .end_byte();
                let ty = declared(node.child_by_field_name("type"));
                push(out, name, ty, node.end_byte(), until, content);
            }
            "declaration" => {
                let ty = node.child_by_field_name("type");
                let until = up.nth(1).unwrap_or(node).end_byte();
                let mut cursor = node.walk();
                for declarator in node.children_by_field_name("declarator", &mut cursor) {
                    let (target, value) = if declarator.kind() == "init_declarator" {
                        (
                            declarator.child_by_field_name("declarator"),
                            declarator.child_by_field_name("value"),
                        )
                    } else {
                        (Some(declarator), None)
                    };
                    let Some(name) = target.and_then(cpp_declarator_name) else {
                        continue;
                    };
                    let resolved = if ty.is_some_and(|ty| ty.kind() == "placeholder_type_specifier")
                    {
                        value.and_then(|value| cpp_constructor(value, content))
                    } else {
                        declared(ty)
                    };
                    push(out, name, resolved, declarator.end_byte(), until, content);
                }
            }
            "for_range_loop" => {
                if let Some(name) = node
                    .child_by_field_name("declarator")
                    .and_then(cpp_declarator_name)
                {
                    let ty = declared(node.child_by_field_name("type"));
                    push(out, name, ty, name.end_byte(), node.end_byte(), content);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn cpp_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "new_expression").then_some(())?;
    clean_type_text(
        node_text(value.child_by_field_name("type")?, content)?,
        Lang::Cpp,
    )
}

/// The name a C++ declarator binds, through pointer and reference
/// declarators; arrays and function declarators bind no typed object.
pub(super) fn cpp_declarator_name(declarator: Node<'_>) -> Option<Node<'_>> {
    let mut current = declarator;
    loop {
        current = match current.kind() {
            "identifier" | "field_identifier" => return Some(current),
            "pointer_declarator" | "reference_declarator" => current
                .child_by_field_name("declarator")
                .or_else(|| current.named_child(0))?,
            _ => return None,
        };
    }
}
