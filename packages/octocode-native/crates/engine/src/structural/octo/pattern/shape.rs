//! How a pattern parsed, as one line: the S-expression of its node's named
//! kinds, field names and metavariables, so a reader compares it with a real
//! node's syntax tree in one step.

use std::fmt::Write as _;

use tree_sitter::Node;

use super::CompiledPattern;
use crate::structural::octo::matching::node_text;
use crate::structural::octo::metavars::{MetaVar, meta_from_node};

impl CompiledPattern {
    /// The pattern's node as `(kind field:(kind "text") $X)`: a named node with
    /// its named children and the anonymous ones a field names, a named node
    /// with none with its text unless that is punctuation alone, a
    /// metavariable as the caller spells it. Every node is rendered; `None`
    /// for a special pattern.
    pub(in crate::structural::octo) fn shape(&self) -> Option<String> {
        let mut out = String::new();
        self.render(self.root()?, &mut out);
        Some(out)
    }

    fn render(&self, node: Node<'_>, out: &mut String) {
        if let Some(meta) = meta_from_node(node, &self.source, self.expando) {
            out.push_str(&match meta {
                MetaVar::Single(name) => format!("${name}"),
                MetaVar::Multi(Some(name)) => format!("$$${name}"),
                MetaVar::IgnoredSingle => "$_".to_owned(),
                MetaVar::IgnoredMulti | MetaVar::Multi(None) => "$$$".to_owned(),
            });
            return;
        }
        let text = || self.expando.restore(node_text(node, &self.source));
        if !node.is_named() {
            let _ = write!(out, "{:?}", text());
            return;
        }
        let _ = write!(out, "({}", node.kind());
        if node.named_child_count() == 0 {
            // A name, keyword or literal reads with its text; punctuation
            // alone (`()`, `{}`) adds nothing to the kind.
            let text = text();
            if text.chars().any(char::is_alphanumeric) {
                let _ = write!(out, " {text:?}");
            }
            out.push(')');
            return;
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                let field = cursor.field_name();
                if child.is_named() || field.is_some() {
                    out.push(' ');
                    if let Some(field) = field {
                        let _ = write!(out, "{field}:");
                    }
                    self.render(child, out);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        out.push(')');
    }
}
