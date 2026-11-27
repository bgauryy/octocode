//! Visibility hints from a declaration's own header, selected by its grammar.

use crate::signatures::node_kinds::kind_of;
use tree_sitter::Node;

use super::nodes::node_text;

#[derive(Clone, Copy)]
pub(crate) enum VisibilityAdapter {
    TopLevel,
    /// JS/TS read by the tree-sitter lane: `export` is written in the source.
    EsModule,
    Rust,
    Go,
    CFamily,
    Java,
    #[cfg(feature = "tree-sitter-asm")]
    None,
    #[cfg(feature = "tree-sitter-scala")]
    Scala,
    #[cfg(feature = "tree-sitter-c-sharp")]
    CSharp,
}

impl VisibilityAdapter {
    pub(super) fn transparent_namespace(self, node: Node<'_>) -> bool {
        matches!(self, Self::CFamily)
            && kind_of(node) == "namespace_definition"
            && node.child_by_field_name("name").is_some()
    }

    pub(super) fn exported(
        self,
        node: Node<'_>,
        ancestors: &[Node<'_>],
        content: &str,
        name: &str,
        parent: Option<&str>,
        public_namespace_scope: bool,
    ) -> bool {
        match self {
            Self::TopLevel => parent.is_none() && !name.starts_with('_'),
            Self::EsModule => parent.is_none() && is_written_export(ancestors),
            Self::Rust => node_text(node, content)
                .unwrap_or("")
                .trim_start()
                .strip_prefix("pub")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace)),
            Self::Go => name.chars().next().is_some_and(char::is_uppercase),
            Self::CFamily => {
                (parent.is_none() || public_namespace_scope)
                    && !(kind_of(node) == "namespace_definition"
                        && node.child_by_field_name("name").is_none())
                    && !header_has(node, content, &["storage_class_specifier"], &["static"])
            }
            Self::Java => {
                header_has(node, content, &["modifiers"], &["public"])
                    && !header_has(node, content, &["modifiers"], &["private", "protected"])
            }
            #[cfg(feature = "tree-sitter-c-sharp")]
            Self::CSharp => {
                header_has(node, content, &["modifier"], &["public"])
                    && !header_has(
                        node,
                        content,
                        &["modifier"],
                        &["private", "protected", "internal"],
                    )
            }
            #[cfg(feature = "tree-sitter-scala")]
            Self::Scala => {
                parent.is_none()
                    && !name.starts_with('_')
                    && !header_has(
                        node,
                        content,
                        &["modifiers", "access_modifier"],
                        &["private", "protected"],
                    )
            }
            #[cfg(feature = "tree-sitter-asm")]
            Self::None => false,
        }
    }
}

/// Whether the declaration under `ancestors` sits in an `export` statement,
/// directly or through the variable statement or `declare` that wraps it.
fn is_written_export(ancestors: &[Node<'_>]) -> bool {
    ancestors
        .iter()
        .rev()
        .find(|parent| {
            !matches!(
                kind_of(*parent),
                "lexical_declaration" | "variable_declaration" | "ambient_declaration"
            )
        })
        .is_some_and(|parent| kind_of(parent) == "export_statement")
}

fn header_has(node: Node<'_>, content: &str, containers: &[&str], keywords: &[&str]) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(|child| {
        if containers.contains(&kind_of(child)) {
            modifier_has(child, content, keywords, 0)
        } else if kind_of(node) == "template_declaration"
            && kind_of(child) != "template_parameter_list"
        {
            header_has(child, content, containers, keywords)
        } else {
            false
        }
    })
}

fn modifier_has(node: Node<'_>, content: &str, keywords: &[&str], depth: u8) -> bool {
    if keywords.contains(&kind_of(node))
        || node_text(node, content).is_some_and(|text| keywords.contains(&text))
    {
        return true;
    }
    if depth >= 2 {
        return false;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        keywords.contains(&kind_of(child))
            || (kind_of(child) == "access_modifier"
                && modifier_has(child, content, keywords, depth + 1))
    })
}
