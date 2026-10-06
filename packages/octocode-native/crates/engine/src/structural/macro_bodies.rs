//! Rust macro arguments are token trees: tree-sitter keeps `assert!(x.unwrap())`
//! as `macro_invocation > token_tree` of flat tokens, so no expression pattern
//! can match inside it. Structural search and rewrite re-parse each macro
//! invocation's token tree (delimiters included, so `(a, b)` reads as a tuple
//! and `{ … }` as a block) with tree-sitter included ranges: node positions
//! stay in file coordinates, and nested invocations are expanded the same way.

use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use super::kinds::named_kind_id;
use super::octo::ExecutionError;

/// Nesting bound for macro bodies inside macro bodies.
pub(super) const MAX_MACRO_DEPTH: usize = 16;

/// Grammar ids for macro expansion; absent for grammars without Rust macros.
#[derive(Clone)]
pub(super) struct MacroBodies {
    macro_invocation: u16,
    token_tree: u16,
    /// Literals one of which every match contains; a body without any of them
    /// cannot match and keeps its flat token tree.
    anchors: Option<Vec<String>>,
}

impl MacroBodies {
    pub(super) fn for_language(language: &Language, anchors: Option<Vec<String>>) -> Option<Self> {
        Some(Self {
            macro_invocation: named_kind_id(language, "macro_invocation")?,
            token_tree: named_kind_id(language, "token_tree")?,
            anchors: anchors.filter(|anchors| !anchors.is_empty()),
        })
    }

    /// Whether `node` is a macro invocation's argument token tree worth re-parsing.
    pub(super) fn is_expandable(
        &self,
        node: Node<'_>,
        parent: Option<Node<'_>>,
        content: &str,
    ) -> bool {
        node.kind_id() == self.token_tree
            && parent.is_some_and(|parent| parent.kind_id() == self.macro_invocation)
            && node.end_byte() > node.start_byte() + 2
            && self.anchors.as_ref().is_none_or(|anchors| {
                let text = content.get(node.byte_range()).unwrap_or_default();
                anchors.iter().any(|anchor| text.contains(anchor.as_str()))
            })
    }

    /// Top-level expandable token trees of `root`, in document order (bodies
    /// nested in them are found when their own re-parse is walked).
    pub(super) fn token_trees(&self, root: Node<'_>, content: &str) -> Vec<tree_sitter::Range> {
        let mut found = Vec::new();
        let mut stack = vec![(root, None::<Node<'_>>)];
        while let Some((node, parent)) = stack.pop() {
            if self.is_expandable(node, parent, content) {
                found.push(node.range());
                continue;
            }
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            stack.extend(children.into_iter().rev().map(|child| (child, Some(node))));
        }
        found
    }
}

/// Parse one token tree as Rust source; `None` when the parser declines it.
pub(super) fn parse_body(
    content: &str,
    language: &Language,
    range: tree_sitter::Range,
    deadline: Instant,
) -> Result<Option<Tree>, ExecutionError> {
    let tree =
        crate::signatures::extractor::parse_ranges_before(content, language, &[range], deadline);
    if tree.is_none() {
        ExecutionError::check(deadline)?;
    }
    Ok(tree)
}

/// Whether a node of a re-parsed body is a real match candidate: not the
/// synthetic wrapper spanning the whole token tree, and not recovered syntax.
pub(super) fn is_body_candidate(node: Node<'_>, body: &tree_sitter::Range) -> bool {
    node.byte_range() != (body.start_byte..body.end_byte) && !node.has_error()
}
