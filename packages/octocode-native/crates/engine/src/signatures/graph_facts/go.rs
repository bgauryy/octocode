//! Go-specific syntax facts: the package a file belongs to, what an import
//! binds, the names a `const` or `var` spec declares, the type a method is
//! declared on, and the types a struct or interface embeds.

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

/// What the lines above a file's declarations settle about it.
#[derive(Default)]
pub(super) struct PackageHeader {
    /// The name its package clause gives the package.
    pub(super) package: Option<String>,
    /// A build constraint keeps the file out of every build.
    pub(super) build_excluded: bool,
}

/// Reads the comments and package clause that open a file. Build constraints
/// count only there.
pub(super) fn package_header(root: Node<'_>, content: &str) -> PackageHeader {
    let mut header = PackageHeader::default();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "comment" => {
                header.build_excluded |=
                    node_text(child, content).is_some_and(constraint_excludes_every_build);
            }
            "package_clause" => {
                let mut clause = child.walk();
                header.package = child
                    .named_children(&mut clause)
                    .find(|part| part.kind() == "package_identifier")
                    .and_then(|name| node_text(name, content))
                    .map(str::to_owned);
                break;
            }
            _ => break,
        }
    }
    header
}

/// A constraint no build satisfies: the `ignore` tag every Go tool chain
/// leaves unset, required on its own or in a conjunction. Whether any other
/// tag holds depends on the build, so those files stay candidates.
fn constraint_excludes_every_build(comment: &str) -> bool {
    let comment = comment.trim();
    if let Some(expression) = comment.strip_prefix("//go:build ") {
        return !expression.contains("||")
            && expression.split("&&").any(|term| term.trim() == "ignore");
    }
    comment
        .strip_prefix("// +build ")
        .or_else(|| comment.strip_prefix("//+build "))
        .is_some_and(|options| {
            options
                .split_whitespace()
                .all(|option| option.split(',').any(|term| term == "ignore"))
        })
}

/// The names a `const_spec` or package-level `var_spec` declares, each with
/// its kind. A function's `var`s are its locals, not the package's.
pub(super) fn spec_names<'t>(
    node: Node<'t>,
    ancestors: &[Node<'t>],
) -> Vec<(&'static str, Node<'t>)> {
    let kind = match node.kind() {
        "const_spec" => "constant",
        "var_spec"
            if ancestors
                .iter()
                .rev()
                .find(|parent| !matches!(parent.kind(), "var_declaration" | "var_spec_list"))
                .is_some_and(|parent| parent.kind() == "source_file") =>
        {
            "variable"
        }
        _ => return Vec::new(),
    };
    let mut cursor = node.walk();
    node.children_by_field_name("name", &mut cursor)
        .filter(Node::is_named)
        .map(|name| (kind, name))
        .collect()
}

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
