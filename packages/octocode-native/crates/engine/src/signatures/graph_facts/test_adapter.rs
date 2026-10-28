//! Test declaration adapters. GoogleTest needs a direct framework include;
//! the model observes syntax and does not evaluate conditional preprocessing.

use tree_sitter::Node;

use crate::graph::{GraphTestIdentity, GraphTestKind};
use crate::signatures::nodes::node_text;

#[derive(Default)]
pub(super) struct TestContext {
    googletest: bool,
    overridden: std::collections::HashSet<String>,
}

impl TestContext {
    pub(super) fn observe(&mut self, node: Node<'_>, source: &str) {
        match node.kind() {
            "preproc_include" => {
                if let Some(path) = node
                    .child_by_field_name("path")
                    .and_then(|n| node_text(n, source))
                {
                    let path = path.trim_matches(['"', '<', '>']);
                    self.googletest |= path == "gtest/gtest.h" || path.ends_with("/gtest/gtest.h");
                }
            }
            "preproc_def" | "preproc_function_def" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| node_text(n, source))
                {
                    self.overridden.insert(name.to_owned());
                }
            }
            "preproc_call" => {
                if node
                    .child_by_field_name("directive")
                    .and_then(|directive| node_text(directive, source))
                    .is_some_and(|directive| {
                        directive
                            .strip_prefix('#')
                            .is_some_and(|name| name.trim() == "undef")
                    })
                    && let Some(argument) = node
                        .child_by_field_name("argument")
                        .and_then(|argument| node_text(argument, source))
                        .and_then(undefined_macro_name)
                {
                    self.overridden.insert(argument.to_owned());
                }
            }
            _ => {}
        }
    }
}

fn undefined_macro_name(mut argument: &str) -> Option<&str> {
    loop {
        argument = argument.trim_start();
        let Some(comment) = argument.strip_prefix("/*") else {
            break;
        };
        let end = comment.find("*/")?;
        argument = &comment[end + 2..];
    }
    let end = argument
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(argument.len());
    (end > 0).then_some(&argument[..end])
}

pub(super) fn test_identity<'t>(
    node: Node<'t>,
    source: &str,
    extension: &str,
    name: Node<'t>,
    context: &TestContext,
) -> Option<(GraphTestIdentity, Node<'t>)> {
    if extension == "rs" && node.kind() == "function_item" && has_test_attribute(node, source) {
        return Some((
            GraphTestIdentity::Rust {
                name: node_text(name, source)?.to_owned(),
                parameterized: false,
                kind: GraphTestKind::Declaration,
            },
            name,
        ));
    }
    if !matches!(extension, "cpp" | "cu") || !context.googletest {
        return None;
    }
    let macro_name = node_text(name, source)?;
    let parameterized = match macro_name {
        "TEST" | "TEST_F" => false,
        "TEST_P" | "TYPED_TEST" | "TYPED_TEST_P" => true,
        _ => return None,
    };
    if context.overridden.contains(macro_name)
        || node.kind() != "function_definition"
        || node.child_by_field_name("type").is_some()
        || node.child_by_field_name("body").is_none()
    {
        return None;
    }
    let declarator = node.child_by_field_name("declarator")?;
    if declarator.has_error()
        || declarator.child_by_field_name("declarator")?.kind() != "identifier"
    {
        return None;
    }
    let parameters = declarator.child_by_field_name("parameters")?;
    let mut cursor = parameters.walk();
    let arguments: Vec<_> = parameters
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect();
    let [suite, test] = arguments.as_slice() else {
        return None;
    };
    let plain = |node: Node<'t>| {
        if node.kind() != "parameter_declaration" || node.named_child_count() != 1 {
            return None;
        }
        node.child_by_field_name("type")
            .filter(|name| name.kind() == "type_identifier")
    };
    let suite = plain(*suite)?;
    let test = plain(*test)?;
    Some((
        GraphTestIdentity::Googletest {
            suite: node_text(suite, source)?.to_owned(),
            name: node_text(test, source)?.to_owned(),
            parameterized,
            kind: GraphTestKind::Declaration,
        },
        test,
    ))
}

fn has_test_attribute(node: Node<'_>, source: &str) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(attribute) = previous {
        match attribute.kind() {
            "attribute_item" => {
                let mut cursor = attribute.walk();
                if attribute.named_children(&mut cursor).any(|inner| {
                    inner.kind() == "attribute"
                        && inner.named_child_count() == 1
                        && inner.named_child(0).is_some_and(|name| {
                            name.kind() == "identifier" && node_text(name, source) == Some("test")
                        })
                }) {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        previous = attribute.prev_named_sibling();
    }
    false
}
