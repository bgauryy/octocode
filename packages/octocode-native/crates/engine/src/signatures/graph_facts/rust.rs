//! Rust-specific graph-fact collectors: `use`-tree expansion, module
//! attributes/scopes, and unsupported-context detection.

use tree_sitter::Node;

use crate::signatures::nodes::{declaration_name, node_text};

use super::{GraphAccumulator, GraphImport, LineIndex};

pub(super) fn rust_module_attributes(node: Node<'_>, content: &str) -> (Option<String>, bool) {
    let mut previous = node.prev_named_sibling();
    let mut path = None;
    let mut unsupported = false;
    while let Some(attribute) = previous {
        if matches!(attribute.kind(), "line_comment" | "block_comment") {
            previous = attribute.prev_named_sibling();
            continue;
        }
        if attribute.kind() != "attribute_item" {
            break;
        }
        let text = node_text(attribute, content).unwrap_or_default();
        let inner = text
            .trim()
            .strip_prefix("#[")
            .and_then(|text| text.strip_suffix(']'))
            .unwrap_or_default()
            .trim();
        let name = inner.split(['(', '=']).next().unwrap_or_default().trim();
        match name {
            "path" => {
                let literal = inner.split_once('=').map(|(_, value)| value.trim());
                let value = literal
                    .and_then(|value| value.strip_prefix('"'))
                    .and_then(|value| value.strip_suffix('"'));
                if let Some(value) =
                    value.filter(|value| !value.contains('\\') && !value.contains('\0'))
                {
                    if path.is_some() {
                        unsupported = true;
                    }
                    path = Some(value.to_owned());
                } else {
                    unsupported = true;
                }
            }
            "allow"
            | "warn"
            | "deny"
            | "forbid"
            | "expect"
            | "doc"
            | "deprecated"
            | "no_implicit_prelude" => {}
            _ => unsupported = true,
        }
        previous = attribute.prev_named_sibling();
    }
    (path, unsupported)
}

pub(super) fn rust_block_local(node: Node<'_>) -> bool {
    let mut parent = node.parent();
    while let Some(scope) = parent {
        if matches!(scope.kind(), "block" | "function_item" | "impl_item") {
            return true;
        }
        parent = scope.parent();
    }
    false
}

pub(super) fn rust_module_scope(node: Node<'_>, content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut parent = node.parent();
    while let Some(scope) = parent {
        if scope.kind() == "mod_item" {
            if let Some(name) = declaration_name(scope, content) {
                names.push(name);
            }
        }
        parent = scope.parent();
    }
    names.reverse();
    names
}

pub(super) fn rust_inner_unsupported(node: Node<'_>, content: &str) -> bool {
    let mut cursor = node.walk();
    let unsupported = node.named_children(&mut cursor).any(|child| {
        if child.kind() != "inner_attribute_item" {
            return false;
        }
        let text = node_text(child, content).unwrap_or_default();
        let inner = text
            .trim()
            .strip_prefix("#![")
            .and_then(|text| text.strip_suffix(']'))
            .unwrap_or_default()
            .trim();
        let name = inner.split(['(', '=']).next().unwrap_or_default().trim();
        !matches!(
            name,
            "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "expect"
                | "doc"
                | "deprecated"
                | "no_implicit_prelude"
        )
    });
    unsupported
}

pub(super) fn rust_unsupported_context(node: Node<'_>, content: &str) -> bool {
    let mut current = Some(node);
    while let Some(scope) = current {
        if rust_module_attributes(scope, content).1 || rust_inner_unsupported(scope, content) {
            return true;
        }
        current = scope.parent();
    }
    false
}

/// Expand Rust use trees through grammar nodes, preserving aliases and multiline groups.
pub(super) fn collect_rust_imports(
    node: Node<'_>,
    module_scope: &[String],
    content: &str,
    index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    unsupported: bool,
    deadline: std::time::Instant,
) {
    let mut pending = vec![(node, String::new())];
    while let Some((node, prefix)) = pending.pop() {
        if std::time::Instant::now() >= deadline {
            return;
        }
        let text = node_text(node, content).unwrap_or_default();
        match node.kind() {
            "use_list" => {
                let mut cursor = node.walk();
                let children_start = pending.len();
                for child in node.named_children(&mut cursor) {
                    pending.push((child, prefix.clone()));
                }
                pending[children_start..].reverse();
            }
            "scoped_use_list" => {
                let path = node
                    .child_by_field_name("path")
                    .and_then(|n| node_text(n, content))
                    .unwrap_or_default();
                let joined = if prefix.is_empty() {
                    path.to_owned()
                } else {
                    format!("{prefix}::{path}")
                };
                if let Some(list) = node.child_by_field_name("list") {
                    pending.push((list, joined));
                }
            }
            _ => {
                let (path_node, alias_node) = if node.kind() == "use_as_clause" {
                    (
                        node.child_by_field_name("path").unwrap_or(node),
                        node.child_by_field_name("alias"),
                    )
                } else {
                    (node, None)
                };
                let path = node_text(path_node, content).unwrap_or(text);
                let alias = alias_node.and_then(|alias| node_text(alias, content));
                let imported_node = path_node.child_by_field_name("name").unwrap_or(path_node);
                let imported_node =
                    matches!(imported_node.kind(), "identifier" | "type_identifier")
                        .then_some(imported_node);
                let specifier = if path == "self" && !prefix.is_empty() {
                    prefix.to_owned()
                } else if prefix.is_empty() {
                    path.to_owned()
                } else {
                    format!("{prefix}::{path}")
                };
                let imported = specifier
                    .rsplit("::")
                    .next()
                    .unwrap_or(&specifier)
                    .to_owned();
                let line = index.range(node).start.line + 1;
                acc.imports.push(GraphImport {
                    id: format!("import:{specifier}:{line}:{}", acc.imports.len()),
                    specifier,
                    line,
                    import_kind: "value",
                    local_name: Some(alias.unwrap_or(&imported).to_owned()),
                    imported_name: Some(imported),
                    imported_range: imported_node.map(|name| index.range(name)),
                    local_range: alias_node.or(imported_node).map(|name| index.range(name)),
                    resolution_hint: unsupported.then_some("unsupported"),
                    module_scope: Some(module_scope.to_vec()),
                });
            }
        }
    }
}
