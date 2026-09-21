use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::time::Instant;

use tree_sitter::{Language, Node, ParseOptions, Parser, Tree};

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use crate::structural::types::StructuralMatch;

use super::line_index_support::{LineIndex, to_structural_match_with_index};

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
    pub(in crate::structural) node_kind: String,
}

impl MatchWithKind {
    pub(super) fn new(node: Node<'_>, matched: StructuralMatch) -> Self {
        Self {
            node_kind: node.kind().to_owned(),
            matched,
        }
    }
}

thread_local! {
    /// Reused across files on each worker thread so the structural walk doesn't
    /// allocate a fresh `Parser` (and its internal scratch buffers) per file —
    /// `search_files` parses one file per candidate, often thousands, in a
    /// `rayon` pool. Each rayon worker gets its own mutable parser without
    /// contending on a shared lock, and reapplies `set_language`: a single-extension
    /// group already shares one grammar, so the call is cheap relative to
    /// constructing a parser from scratch.
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
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
    let interrupted = || {
        ExecutionError::limit(
            "structural.parse.interrupted",
            "parse",
            "Structural parsing exceeded its execution deadline",
        )
    };
    if Instant::now() >= deadline {
        return Err(interrupted());
    }
    PARSER.with(|parser| {
        let mut parser = parser.borrow_mut();
        parser.reset();
        parser.set_language(language).map_err(|err| {
            ExecutionError::limit("structural.parse.failed", "parse", err.to_string())
        })?;
        let bytes = content.as_bytes();
        let mut read = |offset: usize, _| bytes.get(offset..).unwrap_or(b"");
        let mut progress = |_: &tree_sitter::ParseState| {
            if Instant::now() >= deadline {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let tree = parser
            .parse_with_options(
                &mut read,
                None,
                Some(ParseOptions::new().progress_callback(&mut progress)),
            )
            .filter(|_| Instant::now() < deadline);
        if tree.is_none() {
            parser.reset();
        }
        tree.ok_or_else(interrupted)
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

#[derive(Default, Clone)]
pub(super) struct CaptureEnv {
    values: HashMap<String, Vec<String>>,
    ranges: HashMap<String, Vec<RawRange>>,
}

impl CaptureEnv {
    pub(super) fn capture_one(&mut self, name: &str, text: String, range: RawRange) -> bool {
        match self.values.get(name) {
            Some(existing) => existing.as_slice() == [text.as_str()],
            None => {
                self.values.insert(name.to_owned(), vec![text]);
                self.ranges.insert(name.to_owned(), vec![range]);
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
        self.values.insert(name.to_owned(), vec![text]);
        self.ranges.insert(name.to_owned(), vec![range]);
    }

    pub(super) fn capture_many(
        &mut self,
        name: &str,
        texts: Vec<String>,
        ranges: Vec<RawRange>,
    ) -> bool {
        match self.values.get(name) {
            Some(existing) => existing == &texts,
            None => {
                self.values.insert(name.to_owned(), texts);
                self.ranges.insert(name.to_owned(), ranges);
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
}

pub(super) fn children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::with_capacity(node.child_count() as usize);
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            out.push(child);
        }
    }
    out
}

pub(in crate::structural) fn named_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::with_capacity(node.named_child_count());
    for index in 0..node.named_child_count() {
        if let Some(child) = node.named_child(index as u32) {
            out.push(child);
        }
    }
    out
}

pub(in crate::structural) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
}
