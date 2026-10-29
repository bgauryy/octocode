//! Go-specific syntax facts: what an import binds, the type a method is
//! declared on, and the types a struct or interface embeds.

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

/// `(local_name, imported_name)` of an `import_spec`: an alias binds the
/// package under that name, `.` binds its exported names into the file (a
/// glob, `*`), and `_` binds nothing. A plain import binds the package name.
pub(super) fn import_binding(node: Node<'_>, content: &str) -> (Option<String>, Option<String>) {
    let Some(name) = node.child_by_field_name("name") else {
        return (None, None);
    };
    match node_text(name, content) {
        Some(".") => (Some(".".to_owned()), Some("*".to_owned())),
        Some(alias) if !alias.is_empty() => (Some(alias.to_owned()), None),
        _ => (None, None),
    }
}

/// The base type name a `method_declaration` is declared on: `T` for
/// `func (r T) M()`, `func (r *T) M()` and `func (r *T[U]) M()`.
pub(super) fn method_owner(node: Node<'_>, content: &str) -> Option<String> {
    if node.kind() != "method_declaration" {
        return None;
    }
    let receivers = node.child_by_field_name("receiver")?;
    let mut cursor = receivers.walk();
    let receiver = receivers
        .named_children(&mut cursor)
        .find(|child| child.kind() == "parameter_declaration")?;
    let mut ty = receiver.child_by_field_name("type")?;
    loop {
        ty = match ty.kind() {
            "pointer_type" | "parenthesized_type" => ty.named_child(0)?,
            "generic_type" => ty.child_by_field_name("type")?,
            "type_identifier" => return node_text(ty, content).map(str::to_owned),
            _ => return None,
        };
    }
}

/// The embedded field types of a struct `type_spec` and the embedded
/// interfaces of an interface `type_spec`, as their name nodes (`T`,
/// `pkg.T`; the generic type of `T[U]`). Type-set terms (`~int | string`)
/// constrain, they embed nothing.
pub(super) fn embedded_types(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let Some(body) = node.child_by_field_name("type") else {
        return out;
    };
    let mut cursor = body.walk();
    match body.kind() {
        "struct_type" => {
            let Some(fields) = body
                .named_children(&mut cursor)
                .find(|child| child.kind() == "field_declaration_list")
            else {
                return out;
            };
            let mut cursor = fields.walk();
            for field in fields.named_children(&mut cursor) {
                if field.kind() == "field_declaration"
                    && field.child_by_field_name("name").is_none()
                    && let Some(ty) = field.child_by_field_name("type")
                {
                    out.extend(type_name(ty));
                }
            }
        }
        "interface_type" => {
            for element in body.named_children(&mut cursor) {
                if element.kind() == "type_elem" && element.named_child_count() == 1 {
                    out.extend(element.named_child(0).and_then(type_name));
                }
            }
        }
        _ => {}
    }
    out
}

fn type_name(ty: Node<'_>) -> Option<Node<'_>> {
    match ty.kind() {
        "type_identifier" | "qualified_type" => Some(ty),
        "pointer_type" => ty.named_child(0).and_then(type_name),
        "generic_type" => ty.child_by_field_name("type"),
        _ => None,
    }
}
