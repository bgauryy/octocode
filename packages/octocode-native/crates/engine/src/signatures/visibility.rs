//! Visibility hints from a declaration's own header, selected by its grammar.

use tree_sitter::Node;

use super::nodes::node_text;

#[derive(Clone, Copy)]
pub(crate) enum VisibilityAdapter {
    TopLevel,
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
            && node.kind() == "namespace_definition"
            && node.child_by_field_name("name").is_some()
    }

    pub(super) fn exported(
        self,
        node: Node<'_>,
        content: &str,
        name: &str,
        parent: Option<&str>,
        public_namespace_scope: bool,
    ) -> bool {
        match self {
            Self::TopLevel => parent.is_none() && !name.starts_with('_'),
            Self::Rust => node_text(node, content)
                .unwrap_or("")
                .trim_start()
                .strip_prefix("pub")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace)),
            Self::Go => name.chars().next().is_some_and(char::is_uppercase),
            Self::CFamily => {
                (parent.is_none() || public_namespace_scope)
                    && !(node.kind() == "namespace_definition"
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

fn header_has(node: Node<'_>, content: &str, containers: &[&str], keywords: &[&str]) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(|child| {
        if containers.contains(&child.kind()) {
            modifier_has(child, content, keywords, 0)
        } else if node.kind() == "template_declaration" && child.kind() != "template_parameter_list"
        {
            header_has(child, content, containers, keywords)
        } else {
            false
        }
    })
}

fn modifier_has(node: Node<'_>, content: &str, keywords: &[&str], depth: u8) -> bool {
    if keywords.contains(&node.kind())
        || node_text(node, content).is_some_and(|text| keywords.contains(&text))
    {
        return true;
    }
    if depth >= 2 {
        return false;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        keywords.contains(&child.kind())
            || (child.kind() == "access_modifier"
                && modifier_has(child, content, keywords, depth + 1))
    })
}
