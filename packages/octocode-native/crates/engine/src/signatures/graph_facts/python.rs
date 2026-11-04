//! Python-specific graph-fact collector for `import`/`from ... import`
//! statements, preserving module paths and alias bindings. An import in a
//! branch that `typing.TYPE_CHECKING` keeps from running is a `type` import,
//! however the file spells the constant.

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
    let kind = if acc.typing.keeps_from_running(node, ancestors, content) {
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
        acc.typing.observe_import(module, name, alias);
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

/// What `typing.TYPE_CHECKING` goes by in one file, as the walk has met it.
#[derive(Default)]
pub(super) struct TypingBindings {
    /// Local names of the `typing` modules (`t` of `import typing as t`).
    modules: std::collections::HashSet<String>,
    /// Local names of the constant (`TC` of `... import TYPE_CHECKING as TC`,
    /// `FLAG` of `FLAG = TYPE_CHECKING`).
    constants: std::collections::HashSet<String>,
}

const CONSTANT: &str = "TYPE_CHECKING";

fn is_typing_module(name: &str) -> bool {
    matches!(name, "typing" | "typing_extensions")
}

impl TypingBindings {
    /// An imported `name` of `module`, bound as `alias` or under its own name.
    fn observe_import(&mut self, module: Option<&str>, name: &str, alias: Option<&str>) {
        let local = alias.unwrap_or(name);
        match module {
            None if is_typing_module(name) => {
                self.modules.insert(local.to_owned());
            }
            Some(module) if is_typing_module(module) && name == CONSTANT => {
                self.constants.insert(local.to_owned());
            }
            _ => {}
        }
    }

    /// A module-level `name = value`: `name` follows the constant only while
    /// the constant is what it holds.
    pub(super) fn observe_assignment(
        &mut self,
        node: Node<'_>,
        ancestors: &[Node<'_>],
        content: &str,
    ) {
        let [module, statement] = ancestors else {
            return;
        };
        if module.kind() != "module" || statement.kind() != "expression_statement" {
            return;
        }
        let Some(name) = node
            .child_by_field_name("left")
            .filter(|left| left.kind() == "identifier")
            .and_then(|left| node_text(left, content))
        else {
            return;
        };
        let holds_constant = node
            .child_by_field_name("right")
            .is_some_and(|value| self.value_at_run_time(value, content, false) == Some(false));
        if holds_constant {
            self.constants.insert(name.to_owned());
        } else {
            self.constants.remove(name);
        }
    }

    /// What `condition` is when the program runs, where the file settles it:
    /// `TYPE_CHECKING` is false, whatever it is called. `literals` also reads
    /// `True` and `False`, which a condition can test but a variable holding
    /// one may be rebound.
    fn value_at_run_time(
        &self,
        condition: Node<'_>,
        content: &str,
        literals: bool,
    ) -> Option<bool> {
        let operand = |field: &str| {
            condition
                .child_by_field_name(field)
                .and_then(|inner| self.value_at_run_time(inner, content, literals))
        };
        match condition.kind() {
            "identifier" => {
                let name = node_text(condition, content)?;
                (name == CONSTANT || self.constants.contains(name)).then_some(false)
            }
            "attribute" => {
                let object = condition.child_by_field_name("object")?;
                let attribute = condition.child_by_field_name("attribute")?;
                let module = (object.kind() == "identifier")
                    .then(|| node_text(object, content))
                    .flatten()?;
                (node_text(attribute, content) == Some(CONSTANT)
                    && (is_typing_module(module) || self.modules.contains(module)))
                .then_some(false)
            }
            "true" => literals.then_some(true),
            "false" => literals.then_some(false),
            "parenthesized_expression" => {
                self.value_at_run_time(condition.named_child(0)?, content, literals)
            }
            "not_operator" => operand("argument").map(std::ops::Not::not),
            "boolean_operator" => {
                let (left, right) = (operand("left"), operand("right"));
                let text = condition
                    .child_by_field_name("operator")
                    .and_then(|operator| node_text(operator, content));
                let settled = text == Some("or");
                if left == Some(settled) || right == Some(settled) {
                    Some(settled)
                } else {
                    left.filter(|_| right == left)
                }
            }
            _ => None,
        }
    }

    /// Whether the branch holding `node` cannot run: the body of a condition
    /// that is false, or the `else` of one that is true. A function body is
    /// its own run time, so the search ends there.
    fn keeps_from_running(&self, node: Node<'_>, ancestors: &[Node<'_>], content: &str) -> bool {
        let mut child = node;
        for &parent in ancestors.iter().rev() {
            match parent.kind() {
                "if_statement" | "elif_clause" => {
                    let value = parent
                        .child_by_field_name("condition")
                        .and_then(|condition| self.value_at_run_time(condition, content, true));
                    let is_body = parent
                        .child_by_field_name("consequence")
                        .is_some_and(|body| body.id() == child.id());
                    if (is_body && value == Some(false))
                        || (child.kind() == "else_clause" && value == Some(true))
                    {
                        return true;
                    }
                }
                "function_definition" => return false,
                _ => {}
            }
            child = parent;
        }
        false
    }
}
