//! Python-specific graph-fact collector for `import`/`from ... import`
//! statements, preserving module paths and alias bindings.

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

use super::{GraphAccumulator, LineIndex, push_language_import};

pub(super) fn collect_python_imports(
    node: Node<'_>,
    content: &str,
    li: &LineIndex,
    acc: &mut GraphAccumulator,
) {
    let module = node
        .child_by_field_name("module_name")
        .and_then(|module| node_text(module, content));
    let mut cursor = node.walk();
    let names: Vec<_> = node.children_by_field_name("name", &mut cursor).collect();
    let line = li.range(node).start.line + 1;
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
            "value",
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
    if names.is_empty() {
        if let Some(module) = module {
            push_language_import(
                acc,
                module.to_owned(),
                line,
                "value",
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
}
