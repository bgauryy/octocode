//! Registry-owned call observations and the lookup premise their syntax supports.

use tree_sitter::Node;

use crate::graph::{GraphCallBinding, GraphCallBoundary};

use super::nodes::{call_callee, is_call_node, node_text};

#[derive(Clone, Copy)]
pub(crate) enum CallAdapter {
    Ordinary,
    Java,
    #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
    CFamily,
    #[cfg(feature = "tree-sitter-c-sharp")]
    CSharp,
}

pub(super) struct SyntaxCall<'tree> {
    pub callee: String,
    pub name_token: Option<Node<'tree>>,
    pub kind: &'static str,
    pub binding: GraphCallBinding,
}

impl CallAdapter {
    pub(super) fn observe<'tree>(
        self,
        node: Node<'tree>,
        content: &str,
    ) -> Option<SyntaxCall<'tree>> {
        match (self, node.kind()) {
            (Self::Java, "object_creation_expression") => {
                let mut call = named_construction(node, content);
                let mut cursor = node.walk();
                // A qualified instance creation (`outer.new Inner`) needs the
                // receiver's type, which this syntax observation does not establish.
                if node
                    .children(&mut cursor)
                    .take_while(|child| child.kind() != "new")
                    .any(|child| child.is_named() && child.kind() != "comment")
                {
                    call.binding = GraphCallBinding::Unassessed {
                        reason: GraphCallBoundary::UnsupportedConstructorForm,
                    };
                }
                Some(call)
            }
            (Self::Java, "explicit_constructor_invocation") => Some(boundary(
                node,
                content,
                &["this", "super"],
                GraphCallBoundary::ConstructorDelegation,
            )),
            #[cfg(feature = "tree-sitter-c-sharp")]
            (Self::CSharp, "object_creation_expression") => Some(named_construction(node, content)),
            #[cfg(feature = "tree-sitter-c-sharp")]
            (Self::CSharp, "constructor_initializer") => Some(boundary(
                node,
                content,
                &["this", "base"],
                GraphCallBoundary::ConstructorDelegation,
            )),
            #[cfg(feature = "tree-sitter-c-sharp")]
            (Self::CSharp, "implicit_object_creation_expression") => Some(boundary(
                node,
                content,
                &["new"],
                GraphCallBoundary::TargetTypeUnavailable,
            )),
            #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
            (Self::CFamily, "new_expression") => {
                let mut call = named_construction(node, content);
                if node.child_by_field_name("placement").is_some()
                    || node.child_by_field_name("declarator").is_some()
                {
                    call.binding = GraphCallBinding::Unassessed {
                        reason: GraphCallBoundary::UnsupportedConstructorForm,
                    };
                }
                Some(call)
            }
            _ if is_call_node(node.kind()) => {
                let (callee, name_token) = call_callee(node, content)?;
                Some(SyntaxCall {
                    callee,
                    name_token: Some(name_token),
                    kind: "calls",
                    binding: GraphCallBinding::SyntaxLookup,
                })
            }
            _ => None,
        }
    }
}

fn named_construction<'tree>(node: Node<'tree>, content: &str) -> SyntaxCall<'tree> {
    if let Some(ty) = node.child_by_field_name("type")
        && let Some((name, token)) = type_name(ty, content, 0)
    {
        SyntaxCall {
            callee: name,
            name_token: Some(token),
            kind: "constructs",
            binding: GraphCallBinding::ConstructedType,
        }
    } else {
        boundary(
            node,
            content,
            &["new"],
            GraphCallBoundary::UnsupportedConstructorForm,
        )
    }
}

fn boundary<'tree>(
    node: Node<'tree>,
    content: &str,
    keywords: &[&str],
    reason: GraphCallBoundary,
) -> SyntaxCall<'tree> {
    let mut cursor = node.walk();
    let token = node
        .children(&mut cursor)
        .find(|child| !child.is_missing() && keywords.contains(&child.kind()));
    SyntaxCall {
        // The grammar's observed construction keyword is a label, never a lookup target.
        callee: token
            .and_then(|token| node_text(token, content))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| node_text(node, content).unwrap_or_default())
            .to_owned(),
        name_token: token,
        kind: "constructs",
        binding: GraphCallBinding::Unassessed { reason },
    }
}

/// Only name-shaped type nodes are admitted. Type arguments are a separate
/// grammar child and never become target spelling or a fallback argument name.
fn type_name<'tree>(node: Node<'tree>, content: &str, depth: u8) -> Option<(String, Node<'tree>)> {
    if depth >= 32 || node.is_missing() || node.is_error() {
        return None;
    }
    match node.kind() {
        "identifier" | "type_identifier" | "namespace_identifier" => node_text(node, content)
            .filter(|name| !name.is_empty())
            .map(|name| (name.to_owned(), node)),
        "template_type" => type_name(node.child_by_field_name("name")?, content, depth + 1),
        "generic_type" | "generic_name" => {
            let mut cursor = node.walk();
            let name = node.named_children(&mut cursor).find(|child| {
                !matches!(
                    child.kind(),
                    "type_arguments"
                        | "type_argument_list"
                        | "annotation"
                        | "marker_annotation"
                        | "comment"
                )
            })?;
            type_name(name, content, depth + 1)
        }
        "qualified_identifier" | "qualified_name" | "scoped_type_identifier" => {
            let separator = if node.kind() == "qualified_identifier" {
                "::"
            } else {
                "."
            };
            let mut cursor = node.walk();
            let mut parts = Vec::new();
            let mut token = None;
            for child in node.named_children(&mut cursor) {
                if matches!(child.kind(), "comment" | "annotation" | "marker_annotation") {
                    continue;
                }
                let (name, selected) = type_name(child, content, depth + 1)?;
                parts.push(name);
                token = Some(selected);
            }
            token.map(|token| {
                let mut label = parts.join(separator);
                if separator == "::" && node.child(0).is_some_and(|child| child.kind() == "::") {
                    label.insert_str(0, "::");
                }
                (label, token)
            })
        }
        _ => None,
    }
}
