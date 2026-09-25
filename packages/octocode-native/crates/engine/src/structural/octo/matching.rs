use std::collections::HashMap;
use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use crate::signatures::extractor::{AST_EXECUTION_TIMEOUT, ParseFailure, parse_with_deadline};
use crate::structural::types::StructuralMatch;

use super::line_index_support::to_structural_match_with_index;
use crate::text::utf8_offsets::LineIndex;

#[derive(Debug, Clone)]
pub(in crate::structural) struct ExecutionError {
    pub(in crate::structural) code: &'static str,
    pub(in crate::structural) stage: &'static str,
    pub(in crate::structural) message: String,
}
impl ExecutionError {
    pub(in crate::structural) fn from_compile_message(message: &str) -> Option<Self> {
        let detail = message.strip_prefix("[structural.parse.interrupted] ")?;
        Some(Self::limit("structural.parse.interrupted", "parse", detail))
    }
    pub(super) fn check(deadline: Instant) -> Result<(), Self> {
        if Instant::now() >= deadline {
            Err(Self::limit(
                "structural.match.deadline",
                "match",
                "Structural matching exceeded its execution deadline",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn limit(
        code: &'static str,
        stage: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            stage,
            message: message.into(),
        }
    }
    pub(in crate::structural) fn diagnostic(
        &self,
        path: &str,
    ) -> crate::structural::types::StructuralDiagnostic {
        crate::structural::types::StructuralDiagnostic::new(self.code, "warning", self.stage, &self.message)
            .with_path(path).with_recovery("Narrow the search scope or simplify the structural query; this file was not completely evaluated.")
    }
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// A match paired with the tree-sitter `kind` of the node it matched. The
/// non-detailed API discards `node_kind`; the detailed API surfaces it as
/// `StructuralDetailedMatch.node_kind` so callers can see what shape was hit
/// without re-parsing.
pub(in crate::structural) struct MatchWithKind {
    pub(in crate::structural) matched: StructuralMatch,
    pub(in crate::structural) node_kind: NodeKind,
}

impl MatchWithKind {
    pub(super) fn new(node: Node<'_>, matched: StructuralMatch) -> Self {
        Self {
            node_kind: NodeKind {
                language: node.language().to_owned(),
                id: node.kind_id(),
            },
            matched,
        }
    }
}

/// A matched node's kind as a grammar symbol id. The name is only
/// materialized (one `String`) when the detailed API consumes it; the
/// non-detailed API drops it without allocating.
pub(in crate::structural) struct NodeKind {
    language: Language,
    id: u16,
}

impl From<NodeKind> for String {
    fn from(kind: NodeKind) -> Self {
        if kind.id == u16::MAX {
            return "ERROR".to_owned();
        }
        kind.language
            .node_kind_for_id(kind.id)
            .unwrap_or_default()
            .to_owned()
    }
}

#[cfg(test)]
thread_local! {
    pub(in crate::structural) static INTERRUPT_NEXT_COMPILE_PARSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn parse_tree(language: &Language, content: &str) -> Result<Tree, ExecutionError> {
    #[cfg(test)]
    if INTERRUPT_NEXT_COMPILE_PARSE.with(|interrupt| interrupt.replace(false)) {
        return Err(ExecutionError::limit(
            "structural.parse.interrupted",
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
            "structural.parse.interrupted",
            "parse",
            "Structural parsing exceeded its execution deadline",
        ),
    })
}

pub(super) fn visit_named<'tree>(
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

/// [`visit_named`] that also hands each named node its ancestor chain
/// (root first, parent last). The chain is the cursor path, so it costs O(1)
/// per step instead of an O(depth) `Node::parent()` search per ancestor.
pub(super) fn visit_named_with_ancestors<'tree>(
    node: Node<'tree>,
    deadline: Instant,
    f: &mut impl FnMut(Node<'tree>, &[Node<'tree>]) -> Result<(), ExecutionError>,
) -> Result<(), ExecutionError> {
    let mut cursor = node.walk();
    let mut ancestors: Vec<Node<'tree>> = Vec::new();
    loop {
        ExecutionError::check(deadline)?;
        let current = cursor.node();
        if current.is_named() {
            f(current, &ancestors)?;
        }
        if cursor.goto_first_child() {
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
pub(super) fn ancestors_from_root<'tree>(
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

pub(super) fn collect_kind_matches(
    root: Node<'_>,
    kind: &str,
    content: &str,
    deadline: Instant,
) -> Result<Vec<MatchWithKind>, ExecutionError> {
    let line_index = LineIndex::new(content);
    let mut matches = Vec::new();
    visit_named(root, deadline, &mut |candidate| {
        if candidate.kind() == kind {
            matches.push(MatchWithKind::new(
                candidate,
                to_structural_match_with_index(
                    candidate,
                    content,
                    &line_index,
                    HashMap::new(),
                    HashMap::new(),
                ),
            ));
        }
        Ok(())
    })?;
    Ok(matches)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CandidatePlan {
    Any,
    Kinds(Vec<String>),
    Empty,
}

impl CandidatePlan {
    pub(super) fn from_kind(kind: impl Into<String>) -> Self {
        Self::from_kinds([kind.into()])
    }

    pub(super) fn from_kinds(kinds: impl IntoIterator<Item = String>) -> Self {
        let mut kinds = kinds.into_iter().collect::<Vec<_>>();
        kinds.sort();
        kinds.dedup();
        if kinds.is_empty() {
            Self::Empty
        } else {
            Self::Kinds(kinds)
        }
    }

    pub(super) fn matches(&self, candidate: Node<'_>) -> bool {
        self.matches_kind(candidate.kind())
    }

    pub(super) fn matches_kind(&self, kind: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Kinds(kinds) => kinds.iter().any(|candidate| candidate == kind),
            Self::Empty => false,
        }
    }

    pub(super) fn intersect(self, other: Self) -> Self {
        match (self, other) {
            (Self::Empty, _) | (_, Self::Empty) => Self::Empty,
            (Self::Any, plan) | (plan, Self::Any) => plan,
            (Self::Kinds(left), Self::Kinds(right)) => {
                Self::from_kinds(left.into_iter().filter(|kind| right.contains(kind)))
            }
        }
    }

    pub(super) fn union(plans: impl IntoIterator<Item = Self>) -> Self {
        let mut kinds = Vec::new();
        for plan in plans {
            match plan {
                Self::Any => return Self::Any,
                Self::Kinds(plan_kinds) => kinds.extend(plan_kinds),
                Self::Empty => {}
            }
        }
        Self::from_kinds(kinds)
    }
}

/// Raw capture position: (start_row, start_byte_col, end_row, end_byte_col),
/// tree-sitter native. Converted to 1-based line + char column at build time.
pub(super) type RawRange = (u32, u32, u32, u32);

pub(super) fn raw_range(node: Node<'_>) -> RawRange {
    let start = node.start_position();
    let end = node.end_position();
    (
        start.row as u32,
        start.column as u32,
        end.row as u32,
        end.column as u32,
    )
}

/// Internal capture name for relational bookkeeping (`has`/`inside`). Lowercase
/// is unreachable by user metavars — `pre_process_pattern` only treats `A-Z`/`_`
/// after `$` as a metavar — so stripping this key from output can never drop a
/// user capture.
pub(super) const SECONDARY_CAPTURE: &str = "secondary";

/// Metavariable bindings for one candidate match, with an undo log.
///
/// Branching matchers (`has`/`inside`/`all`/`any`/`not`, `$$$` split points,
/// child lists) do not clone the environment per branch. Instead each
/// branch takes a [`checkpoint`](Self::checkpoint) and, on failure,
/// [`rollback`](Self::rollback)s to it: bindings are append-only per branch,
/// so undoing them is O(bindings added since the checkpoint) and a successful
/// branch costs nothing.
#[derive(Default)]
pub(super) struct CaptureEnv {
    values: HashMap<String, Vec<String>>,
    ranges: HashMap<String, Vec<RawRange>>,
    undo: Vec<Undo>,
}

enum Undo {
    /// `name` was newly bound: rollback removes it.
    Insert(String),
    /// `name` was rebound (`capture_replace`): rollback restores the old value.
    Replace(String, Vec<String>, Vec<RawRange>),
}

/// Position in the undo log returned by [`CaptureEnv::checkpoint`].
#[derive(Clone, Copy)]
pub(super) struct Checkpoint(usize);

impl CaptureEnv {
    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.undo.len())
    }

    /// Undo every binding made since `checkpoint`.
    pub(super) fn rollback(&mut self, checkpoint: Checkpoint) {
        while self.undo.len() > checkpoint.0 {
            match self.undo.pop() {
                Some(Undo::Insert(name)) => {
                    self.values.remove(&name);
                    self.ranges.remove(&name);
                }
                Some(Undo::Replace(name, values, ranges)) => {
                    self.values.insert(name.clone(), values);
                    self.ranges.insert(name, ranges);
                }
                None => break,
            }
        }
    }

    fn insert(&mut self, name: &str, values: Vec<String>, ranges: Vec<RawRange>) {
        self.values.insert(name.to_owned(), values);
        self.ranges.insert(name.to_owned(), ranges);
        self.undo.push(Undo::Insert(name.to_owned()));
    }

    /// Bind `name` to `text`, or check it equals the existing binding
    /// (backreference). Allocates only when binding a new name.
    pub(super) fn capture_one(&mut self, name: &str, text: &str, range: RawRange) -> bool {
        match self.values.get(name) {
            Some(existing) => existing.len() == 1 && existing[0] == text,
            None => {
                self.insert(name, vec![text.to_owned()], vec![range]);
                true
            }
        }
    }

    /// Bookkeeping capture for relational rules (`has`/`inside` record the
    /// related node as "secondary"). Unlike user metavars, it carries no
    /// backreference semantics: nested relations each match a different node,
    /// so consistency-checking it (capture_one) rejects valid matches — the
    /// nearest relation simply wins.
    pub(super) fn capture_replace(&mut self, name: &str, text: String, range: RawRange) {
        let old_values = self.values.insert(name.to_owned(), vec![text]);
        let old_ranges = self.ranges.insert(name.to_owned(), vec![range]);
        self.undo.push(match old_values {
            Some(values) => Undo::Replace(name.to_owned(), values, old_ranges.unwrap_or_default()),
            None => Undo::Insert(name.to_owned()),
        });
    }

    /// Bind `name` to a node sequence, or check it equals the existing binding.
    pub(super) fn capture_many<'s>(
        &mut self,
        name: &str,
        texts: impl ExactSizeIterator<Item = &'s str> + Clone,
        ranges: impl FnOnce() -> Vec<RawRange>,
    ) -> bool {
        match self.values.get(name) {
            Some(existing) => {
                existing.len() == texts.len()
                    && existing
                        .iter()
                        .zip(texts)
                        .all(|(left, right)| left == right)
            }
            None => {
                let texts = texts.map(str::to_owned).collect();
                self.insert(name, texts, ranges());
                true
            }
        }
    }

    pub(super) fn into_maps(
        mut self,
    ) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<RawRange>>) {
        self.values.remove(SECONDARY_CAPTURE);
        self.ranges.remove(SECONDARY_CAPTURE);
        (self.values, self.ranges)
    }

    #[cfg(test)]
    pub(super) fn undo_len(&self) -> usize {
        self.undo.len()
    }
}

// Cursor iteration is O(k); `node.child(i)` scans siblings linearly, so an
// indexed loop is O(k²) on wide nodes (long argument lists, big arrays).
pub(super) fn children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    node.children(&mut node.walk()).collect()
}

pub(in crate::structural) fn named_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    node.named_children(&mut node.walk()).collect()
}

pub(in crate::structural) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
}
