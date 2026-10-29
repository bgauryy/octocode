//! Python-specific graph-fact collector for `import`/`from ... import`
//! statements, preserving module paths and alias bindings. An import in the
//! body of `if TYPE_CHECKING:` never runs: it is a `type` import.

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

use super::{GraphAccumulator, NodePositions, push_language_import};

pub(super) fn collect_python_imports(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    li: &NodePositions,
    acc: &mut GraphAccumulator,
) {
    let module = node
        .child_by_field_name("module_name")
        .and_then(|module| node_text(module, content));
    let mut cursor = node.walk();
    let names: Vec<_> = node.children_by_field_name("name", &mut cursor).collect();
    let line = li.range(node).start.line + 1;
    let kind = if type_checking_only(node, ancestors, content) {
        "type"
    } else {
        "value"
    };
    for item in &names {
        let name_node = item.child_by_field_name("name").unwrap_or(*item);
        let Some(name) = node_text(name_node, content) else {
            continue;
        };
        let alias_node = item.child_by_field_name("alias");
        let alias = alias_node.and_then(|alias| node_text(alias, content));
        let specifier = module.unwrap_or(name);
        let binding = push_language_import(
            acc,
            specifier.to_owned(),
            line,
            kind,
            Some(
                alias
                    .unwrap_or_else(|| {
                        if module.is_some() {
                            name
                        } else {
                            name.split('.').next().unwrap_or(name)
                        }
                    })
                    .to_owned(),
            ),
            Some(if module.is_some() { name } else { "*" }.to_owned()),
            if specifier.starts_with('.') {
                "python-relative"
            } else {
                "python-absolute"
            },
        );
        // Synthetic module imports ("*") have no imported-name token.
        binding.imported_range = module.map(|_| li.range(name_node));
        let local_node = alias_node.unwrap_or_else(|| {
            if module.is_some() {
                name_node
            } else {
                name_node.named_child(0).unwrap_or(name_node)
            }
        });
        binding.local_range = Some(li.range(local_node));
    }
    if names.is_empty()
        && let Some(module) = module
    {
        push_language_import(
            acc,
            module.to_owned(),
            line,
            kind,
            Some("*".to_owned()),
            Some("*".to_owned()),
            if module.starts_with('.') {
                "python-relative"
            } else {
                "python-absolute"
            },
        );
    }
}

/// Whether `node` sits in the body of an `if TYPE_CHECKING:` (or
/// `typing.TYPE_CHECKING`) block, not in its `else`/`elif` branches.
fn type_checking_only(node: Node<'_>, ancestors: &[Node<'_>], content: &str) -> bool {
    let mut child = node;
    for &parent in ancestors.iter().rev() {
        if parent.kind() == "if_statement"
            && parent
                .child_by_field_name("consequence")
                .is_some_and(|body| body.id() == child.id())
            && parent
                .child_by_field_name("condition")
                .and_then(|condition| node_text(condition, content))
                .is_some_and(|condition| {
                    matches!(condition.trim(), "TYPE_CHECKING" | "typing.TYPE_CHECKING")
                })
        {
            return true;
        }
        if parent.kind() == "function_definition" {
            return false;
        }
        child = parent;
    }
    false
}
