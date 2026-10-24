//! Parsing with a deadline and walking named nodes, Rust macro bodies included.

use std::collections::HashMap;
use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use crate::signatures::extractor::{AST_EXECUTION_TIMEOUT, ParseFailure, parse_with_deadline};
use crate::structural::octo::macro_bodies::{MAX_MACRO_DEPTH, MacroBodies};

use crate::structural::octo::line_index_support::to_structural_match_with_index;
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
    parse_tree_with_deadline(language, content, Instant::now() + AST_EXECUTION_TIMEOUT)
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
    })
}

pub(in crate::structural::octo) fn visit_named<'tree>(
    node: Node<'tree>,
    deadline: Instant,
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

/// A candidate callback that works on the file tree and on every re-parsed
/// macro body: `(candidate, ancestors root-first, root of candidate's tree)`.
pub(in crate::structural::octo) type CandidateVisitor<'f> =
    dyn for<'n> FnMut(Node<'n>, &[Node<'n>], Node<'n>) -> Result<(), ExecutionError> + 'f;

/// [`visit_named`] that also hands each named node its ancestor chain (the
/// cursor path: O(1) per step, not an O(depth) `Node::parent()` search) and
/// walks Rust macro bodies: an expandable token tree is visited itself, then re-parsed and walked in
/// place of its flat tokens. Ancestors of a body node continue the chain of
/// its invocation, so `inside` rules see the enclosing items.
pub(in crate::structural::octo) fn visit_named_expanding_macros(
    root: Node<'_>,
    content: &str,
    language: &Language,
    macros: Option<&MacroBodies>,
    deadline: Instant,
    f: &mut CandidateVisitor<'_>,
) -> Result<(), ExecutionError> {
    walk_expanding(
        root,
        &[],
        None,
        &[],
        0,
        content,
        language,
        macros,
        deadline,
        f,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the recursive walk threads the macro-body frame and the file context flat, like the matcher (see clippy.toml)"
)]
fn walk_expanding<'tree>(
    root: Node<'tree>,
    prefix: &[Node<'tree>],
    body: Option<&tree_sitter::Range>,
    strings: &[std::ops::Range<usize>],
    depth: usize,
    content: &str,
    language: &Language,
    macros: Option<&MacroBodies>,
    deadline: Instant,
    f: &mut CandidateVisitor<'_>,
) -> Result<(), ExecutionError> {
    let mut cursor = root.walk();
    let mut ancestors: Vec<Node<'tree>> = prefix.to_vec();
    loop {
        ExecutionError::check(deadline)?;
        let current = cursor.node();
        let candidate = match (body, macros) {
            (Some(body), Some(macros)) => macros.is_candidate(current, body, strings),
            _ => true,
        };
        if current.is_named() && candidate {
            f(current, &ancestors, root)?;
        }
        let mut descend = true;
        if let Some(macros) = macros
            && depth < MAX_MACRO_DEPTH
            && macros.is_expandable(current, ancestors.last().copied(), content)
        {
            let range = current.range();
            if let Some(tree) = macros.parse(content, language, range, deadline)? {
                let mut chain: Vec<Node<'_>> = ancestors.clone();
                chain.push(current);
                let body_strings = crate::structural::octo::macro_bodies::string_literals(current);
                walk_expanding(
                    tree.root_node(),
                    &chain,
                    Some(&range),
                    &body_strings,
                    depth + 1,
                    content,
                    language,
                    Some(macros),
                    deadline,
                    f,
                )?;
                descend = false;
            }
        }
        if descend && cursor.goto_first_child() {
            ancestors.push(current);
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(());
            }
            ancestors.pop();
        }
    }
}

/// Ancestor chain of `node` (root first, parent last) computed with one
/// top-down descent from `root`, the same search `ts_node_parent` performs for
/// a single step. Used when a caller has no cursor path for `node`.
pub(in crate::structural::octo) fn ancestors_from_root<'tree>(
    root: Node<'tree>,
    node: Node<'tree>,
    deadline: Instant,
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
    root: Node<'_>,
    kind: &str,
    content: &str,
    language: &Language,
    macros: Option<&MacroBodies>,
    deadline: Instant,
    found: &mut Found<'_>,
) -> Result<(), ExecutionError> {
    let line_index = LineIndex::tree_sitter(content);
    visit_named_expanding_macros(
        root,
        content,
        language,
        macros,
        deadline,
        &mut |candidate, _, _| {
            if candidate.kind() == kind {
                found.push(candidate, || {
                    to_structural_match_with_index(
                        candidate,
                        content,
                        &line_index,
                        HashMap::new(),
                        HashMap::new(),
                    )
                });
            }
            Ok(())
        },
    )
}
