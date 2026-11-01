use crate::num::saturating_u32;
use std::time::Instant;

use tree_sitter::{Language, Node};

use crate::structural::types::{
    FileMatches, StructuralDemand, StructuralDetailedMatch, StructuralMatch,
};

mod capture;
mod walk;

pub(super) use capture::CaptureEnv;
#[cfg(test)]
pub(in crate::structural) use walk::INTERRUPT_NEXT_COMPILE_PARSE;
pub(in crate::structural) use walk::parse_tree_with_deadline;
pub(super) use walk::{
    CandidateVisitor, ancestors_from_root, collect_kind_matches, parse_tree, visit_named,
    visit_named_expanding_macros,
};

/// The code of a parse cut short by its deadline or cancellation.
pub const PARSE_INTERRUPTED: &str = "structural.parse.interrupted";
/// The code of a match cut short by its execution deadline.
pub const MATCH_DEADLINE: &str = "structural.match.deadline";

#[derive(Debug, Clone)]
pub(in crate::structural) struct ExecutionError {
    pub(in crate::structural) code: &'static str,
    pub(in crate::structural) stage: &'static str,
    pub(in crate::structural) message: String,
}
/// When matching stops. Production matching runs each file to completion
/// ([`Deadline::UNBOUNDED`]) and reads no clock; a bound one is checked at
/// every step.
#[derive(Clone, Copy, Debug)]
pub(in crate::structural) struct Deadline(Option<Instant>);

impl Deadline {
    pub(in crate::structural) const UNBOUNDED: Self = Self(None);
    #[cfg(test)]
    pub(in crate::structural) fn at(instant: Instant) -> Self {
        Self(Some(instant))
    }
    /// The deadline of a parse this match runs: an unbounded match parses
    /// the whole file.
    pub(in crate::structural) fn parse_by(self) -> Instant {
        self.0
            .unwrap_or_else(crate::signatures::extractor::whole_file_deadline)
    }
}

impl ExecutionError {
    pub(in crate::structural) fn check(deadline: Deadline) -> Result<(), Self> {
        if deadline.0.is_some_and(|at| Instant::now() >= at) {
            Err(Self::limit(
                MATCH_DEADLINE,
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

/// What a matcher keeps of the matches it finds, by the caller's
/// [`StructuralDemand`]: a match is built only when it is kept or priced.
/// It also records whether the file parsed with recovered syntax that could
/// hide a match ([`super::recovery`]).
pub(in crate::structural) struct Found<'a> {
    source: Option<crate::structural::SourceBinder<'a>>,
    source_error: Option<crate::text::analysis::SourceError>,
    kept: Kept<'a>,
    recovered: bool,
    /// Which 1-based start lines a scope keeps; `None` keeps every match.
    keep_line: Option<&'a (dyn Fn(u32) -> bool + Sync)>,
}

enum Kept<'a> {
    Matches(Vec<MatchWithKind>),
    Priced(
        &'a (dyn Fn(&StructuralDetailedMatch) -> u32 + Sync),
        Vec<u32>,
    ),
    Counted(u32),
}

impl<'a> Found<'a> {
    pub(in crate::structural) fn new(demand: StructuralDemand<'a>) -> Self {
        let kept = match demand {
            StructuralDemand::Matches => Kept::Matches(Vec::new()),
            StructuralDemand::Count => Kept::Counted(0),
            StructuralDemand::Priced(price) => Kept::Priced(price, Vec::new()),
        };
        Self {
            kept,
            source: None,
            source_error: None,
            recovered: false,
            keep_line: None,
        }
    }

    /// Keep only the matches whose 1-based start line `keep` admits.
    pub(in crate::structural) fn keep_lines(&mut self, keep: &'a (dyn Fn(u32) -> bool + Sync)) {
        self.keep_line = Some(keep);
    }

    pub(in crate::structural) fn bind_source(
        &mut self,
        source: crate::structural::SourceBinder<'a>,
    ) {
        self.source = Some(source);
    }
    pub(in crate::structural) fn source_error(
        &self,
    ) -> Option<&crate::text::analysis::SourceError> {
        self.source_error.as_ref()
    }

    /// One match of `node`; `build` makes it only when this demand keeps
    /// or prices it.
    pub(super) fn push(&mut self, node: Node<'_>, build: impl FnOnce() -> StructuralMatch) {
        if self.source_error.is_some() {
            return;
        }
        if let Some(keep) = self.keep_line {
            let line = saturating_u32(node.start_position().row).saturating_add(1);
            if !keep(line) {
                return;
            }
        }
        let build = || {
            let mut matched = build();
            if let Some(source) = &mut self.source {
                source.bind_raw_match(&mut matched)?;
            }
            Ok::<_, crate::text::analysis::SourceError>(matched)
        };
        match &mut self.kept {
            Kept::Matches(matches) => match build() {
                Ok(matched) => matches.push(MatchWithKind::new(node, matched)),
                Err(error) => self.source_error = Some(error),
            },
            Kept::Priced(price, prices) => {
                let matched = match build() {
                    Ok(matched) => matched,
                    Err(error) => {
                        self.source_error = Some(error);
                        return;
                    }
                };
                let found = MatchWithKind::new(node, matched);
                prices.push(price(&StructuralDetailedMatch::from_match(
                    found.matched,
                    found.node_kind,
                )));
            }
            Kept::Counted(count) => *count = count.saturating_add(1),
        }
    }

    /// The file parsed with recovered syntax that could hide a match.
    pub(super) fn mark_recovered(&mut self) {
        self.recovered = true;
    }

    /// Whether the file parsed with recovered syntax that could hide a match.
    pub(in crate::structural) fn recovered(&self) -> bool {
        self.recovered
    }

    /// The kept matches (empty for a count or a price demand).
    pub(in crate::structural) fn into_matches(self) -> Vec<MatchWithKind> {
        match self.kept {
            Kept::Matches(matches) => matches,
            Kept::Priced(..) | Kept::Counted(_) => Vec::new(),
        }
    }

    /// What a file keeps, as the detailed shape.
    pub(in crate::structural) fn into_file_matches(self) -> FileMatches {
        match self.kept {
            Kept::Matches(matches) => FileMatches::Held(
                matches
                    .into_iter()
                    .map(|m| StructuralDetailedMatch::from_match(m.matched, m.node_kind))
                    .collect(),
            ),
            Kept::Priced(_, prices) => FileMatches::Priced(prices),
            Kept::Counted(count) => FileMatches::Counted(count),
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
pub(super) type RawRange = (u32, u32, u32, u32, u32, u32, usize);

pub(super) fn raw_range(node: Node<'_>) -> RawRange {
    let start = node.start_position();
    let end = node.end_position();
    (
        saturating_u32(start.row),
        saturating_u32(start.column),
        saturating_u32(end.row),
        saturating_u32(end.column),
        saturating_u32(node.start_byte()),
        saturating_u32(node.end_byte()),
        super::source_context::tree_id(node),
    )
}

/// Internal capture name for relational bookkeeping (`has`/`inside`). Lowercase
/// is unreachable by user metavars — `pre_process_pattern` only treats `A-Z`/`_`
/// after `$` as a metavar — so stripping this key from output can never drop a
/// user capture.
pub(super) const SECONDARY_CAPTURE: &str = "secondary";

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
