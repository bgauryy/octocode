//! Parsing with a deadline and walking named nodes.

use std::collections::HashMap;
use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use crate::signatures::extractor::{ParseFailure, parse_with_deadline, whole_file_deadline};
use crate::structural::octo::expanded::{ExpandedTree, Seen};
use crate::structural::octo::line_index_support::to_structural_match_with_context;
use crate::text::LineIndex;

use super::{ExecutionError, Found, PARSE_INTERRUPTED};

#[cfg(test)]
thread_local! {
    pub(in crate::structural) static INTERRUPT_NEXT_COMPILE_PARSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(in crate::structural::octo) fn parse_tree(
    language: &Language,
    content: &str,
) -> Result<Tree, ExecutionError> {
    #[cfg(test)]
    if INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.replace(false)) {
        return Err(ExecutionError::limit(
            PARSE_INTERRUPTED,
            "parse",
            "Injected compile parser interruption",
        ));
    }
    parse_tree_with_deadline(language, content, whole_file_deadline())
}

pub(in crate::structural) fn parse_tree_with_deadline(
    language: &Language,
    content: &str,
    deadline: Instant,
) -> Result<Tree, ExecutionError> {
    parse_with_deadline(content, language, deadline).map_err(|failure| match failure {
        ParseFailure::Language(message) => {
            ExecutionError::limit("structural.parse.failed", "parse", message)
        }
        ParseFailure::Interrupted => ExecutionError::limit(
            PARSE_INTERRUPTED,
            "parse",
            "Structural parsing exceeded its execution deadline",
        ),
        ParseFailure::Unbounded => ExecutionError::limit(
            "structural.parse.failed",
            "parse",
            "The grammar's error recovery stopped being linear in this file, so it was not parsed",
        ),
    })
}

pub(in crate::structural::octo) fn visit_named<'tree>(
    node: Node<'tree>,
    deadline: super::Deadline,
    f: &mut impl FnMut(Node<'tree>) -> Result<(), ExecutionError>,
) -> Result<(), ExecutionError> {
    let mut cursor = node.walk();
    loop {
        ExecutionError::check(deadline)?;
        let current = cursor.node();
        if current.is_named() {
            f(current)?;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(());
            }
        }
    }
}

/// Ancestor chain of `node` (root first, parent last) computed with one
/// top-down descent from `root`, the same search `ts_node_parent` performs for
/// a single step. Used when a caller has no cursor path for `node`.
pub(in crate::structural::octo) fn ancestors_from_root<'tree>(
    root: Node<'tree>,
    node: Node<'tree>,
    deadline: super::Deadline,
) -> Result<Vec<Node<'tree>>, ExecutionError> {
    let mut chain = Vec::new();
    let mut current = root;
    while current.id() != node.id() {
        ExecutionError::check(deadline)?;
        let Some(next) = current.child_with_descendant(node) else {
            break;
        };
        chain.push(current);
        if next.id() == current.id() {
            break;
        }
        current = next;
    }
    Ok(chain)
}

pub(in crate::structural::octo) fn collect_kind_matches(
    tree: &ExpandedTree<'_>,
    kind: &str,
    content: &str,
    deadline: super::Deadline,
    found: &mut Found<'_>,
) -> Result<(), ExecutionError> {
    let line_index = LineIndex::tree_sitter(content);
    tree.visit_nodes(deadline, &mut |candidate, _, seen| {
        if candidate.kind() != kind {
            return Ok(());
        }
        if seen == Seen::Unreadable {
            if let Some(body) = tree.disclosed_body(candidate) {
                found.mark_recovered_body(body);
            }
            return Ok(());
        }
        {
            let source = tree.source_for(candidate, [])?;
            found.push(candidate, || {
                to_structural_match_with_context(
                    &source,
                    candidate,
                    content,
                    &line_index,
                    HashMap::new(),
                    HashMap::new(),
                )
            });
        }
        Ok(())
    })
}
