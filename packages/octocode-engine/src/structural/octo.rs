use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use regex::Regex;
use serde::Deserialize;
use std::ops::ControlFlow;
use std::time::Instant;
use tree_sitter::{Language, Node, ParseOptions, Parser, Tree};

use super::language::{AgLanguage, Expando};
use super::query::StructuralQuery;
use super::types::{MetavarRange, StructuralMatch};

pub(super) type OctoCompiledMatcher =
    Box<dyn Fn(&str) -> Result<Vec<MatchWithKind>, ExecutionError> + Send + Sync>;

#[derive(Debug, Clone)]
pub(super) struct ExecutionError {
    pub(super) code: &'static str,
    pub(super) stage: &'static str,
    pub(super) message: String,
}
impl ExecutionError {
    pub(super) fn from_compile_message(message: &str) -> Option<Self> {
        let detail = message.strip_prefix("[structural.parse.interrupted] ")?;
        Some(Self::limit("structural.parse.interrupted", "parse", detail))
    }
    fn check(deadline: Instant) -> Result<(), Self> {
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
    fn limit(code: &'static str, stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            stage,
            message: message.into(),
        }
    }
    pub(super) fn diagnostic(&self, path: &str) -> super::types::StructuralDiagnostic {
        super::types::StructuralDiagnostic::new(self.code, "warning", self.stage, &self.message)
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
pub(super) struct MatchWithKind {
    pub(super) matched: StructuralMatch,
    pub(super) node_kind: String,
}

impl MatchWithKind {
    fn new(node: Node<'_>, matched: StructuralMatch) -> Self {
        Self {
            node_kind: node.kind().to_owned(),
            matched,
        }
    }
}

/// Native recursion guard for pattern matching; AST traversal is iterative.
const MAX_STRUCTURAL_DEPTH: usize = 500;

/// Budget on `$$$` multi-metavar split attempts within a single top-level
/// child-list match. Each `$$$` tries every split point (`for take in
/// 0..=max_take`) and several `$$$` against a wide node are combinatorial; this
/// caps the total work so a crafted pattern + wide input can't stall for
/// seconds. Exhaustion is an explicit incomplete execution error.
const MAX_MULTI_CAPTURE_ATTEMPTS: usize = 10_000;

struct MatchBudget {
    attempts: usize,
    deadline: Instant,
}

pub(super) fn compile_matcher(
    lang: &AgLanguage,
    query: &StructuralQuery<'_>,
) -> Result<OctoCompiledMatcher, String> {
    compile_matcher_inner(lang, query).map_err(|message| {
        if message.starts_with("[structural.") {
            message
        } else {
            format!("[structural.query.compileFailed] {message}")
        }
    })
}

fn compile_matcher_inner(
    lang: &AgLanguage,
    query: &StructuralQuery<'_>,
) -> Result<OctoCompiledMatcher, String> {
    let language = lang.tree_sitter_language();
    match query.parts() {
        (Some(pattern), None) if is_document_probe(pattern) => Ok(Box::new(move |content| {
            let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
            parse_tree_with_deadline(&language, content, deadline).map(|tree| {
                let root = tree.root_node();
                vec![MatchWithKind::new(
                    root,
                    to_structural_match(root, content, HashMap::new(), HashMap::new()),
                )]
            })
        })),
        (Some(pattern), None) => {
            let compiled = CompiledPattern::new(lang, pattern)?;
            Ok(Box::new(move |content| {
                let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
                if compiled.is_special() {
                    return compiled.find_special_matches(content, deadline);
                }

                let tree = parse_tree_with_deadline(compiled.language(), content, deadline)?;
                let line_index = LineIndex::new(content);
                let mut matches = Vec::new();
                visit_named(tree.root_node(), deadline, &mut |candidate| {
                    if !compiled.matches_candidate(candidate) {
                        return Ok(());
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, content, &mut captures, deadline)? {
                        let (values, ranges) = captures.into_maps();
                        matches.push(MatchWithKind::new(
                            candidate,
                            to_structural_match_with_index(
                                candidate,
                                content,
                                &line_index,
                                values,
                                ranges,
                            ),
                        ));
                    }
                    Ok(())
                })?;
                Ok(matches)
            }))
        }
        (None, Some(_)) => {
            let compiled = CompiledRule::compile(lang, query.parsed_rule()?)?;
            let language = lang.tree_sitter_language();
            if let Some(kind) = compiled.simple_kind().map(str::to_owned) {
                return Ok(Box::new(move |content| {
                    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
                    let tree = parse_tree_with_deadline(&language, content, deadline)?;
                    collect_kind_matches(tree.root_node(), &kind, content, deadline)
                }));
            }
            Ok(Box::new(move |content| {
                let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
                let tree = parse_tree_with_deadline(&language, content, deadline)?;
                let document = Document { content, deadline };
                let line_index = LineIndex::new(content);
                let mut matches = Vec::new();
                visit_named(tree.root_node(), deadline, &mut |candidate| {
                    if !compiled.matches_candidate(candidate) {
                        return Ok(());
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, &document, &mut captures)? {
                        let (values, ranges) = captures.into_maps();
                        matches.push(MatchWithKind::new(
                            candidate,
                            to_structural_match_with_index(
                                candidate,
                                content,
                                &line_index,
                                values,
                                ranges,
                            ),
                        ));
                    }
                    Ok(())
                })?;
                Ok(matches)
            }))
        }
        _ => unreachable!("StructuralQuery validates the query shape"),
    }
}

fn is_document_probe(pattern: &str) -> bool {
    pattern.trim() == "$$$"
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
    pub(super) static INTERRUPT_NEXT_COMPILE_PARSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn parse_tree(language: &Language, content: &str) -> Result<Tree, ExecutionError> {
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

fn parse_tree_with_deadline(
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
        let mut read = |offset: usize, _| &bytes[offset..];
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

fn visit_named<'tree>(
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

fn collect_kind_matches(
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
enum CandidatePlan {
    Any,
    Kinds(Vec<String>),
    Empty,
}

impl CandidatePlan {
    fn from_kind(kind: impl Into<String>) -> Self {
        Self::from_kinds([kind.into()])
    }

    fn from_kinds(kinds: impl IntoIterator<Item = String>) -> Self {
        let mut kinds = kinds.into_iter().collect::<Vec<_>>();
        kinds.sort();
        kinds.dedup();
        if kinds.is_empty() {
            Self::Empty
        } else {
            Self::Kinds(kinds)
        }
    }

    fn matches(&self, candidate: Node<'_>) -> bool {
        self.matches_kind(candidate.kind())
    }

    fn matches_kind(&self, kind: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Kinds(kinds) => kinds.iter().any(|candidate| candidate == kind),
            Self::Empty => false,
        }
    }

    fn intersect(self, other: Self) -> Self {
        match (self, other) {
            (Self::Empty, _) | (_, Self::Empty) => Self::Empty,
            (Self::Any, plan) | (plan, Self::Any) => plan,
            (Self::Kinds(left), Self::Kinds(right)) => {
                Self::from_kinds(left.into_iter().filter(|kind| right.contains(kind)))
            }
        }
    }

    fn union(plans: impl IntoIterator<Item = Self>) -> Self {
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
type RawRange = (u32, u32, u32, u32);

fn raw_range(node: Node<'_>) -> RawRange {
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
const SECONDARY_CAPTURE: &str = "secondary";

#[derive(Default, Clone)]
struct CaptureEnv {
    values: HashMap<String, Vec<String>>,
    ranges: HashMap<String, Vec<RawRange>>,
}

impl CaptureEnv {
    fn capture_one(&mut self, name: &str, text: String, range: RawRange) -> bool {
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
    fn capture_replace(&mut self, name: &str, text: String, range: RawRange) {
        self.values.insert(name.to_owned(), vec![text]);
        self.ranges.insert(name.to_owned(), vec![range]);
    }

    fn capture_many(&mut self, name: &str, texts: Vec<String>, ranges: Vec<RawRange>) -> bool {
        match self.values.get(name) {
            Some(existing) => existing == &texts,
            None => {
                self.values.insert(name.to_owned(), texts);
                self.ranges.insert(name.to_owned(), ranges);
                true
            }
        }
    }

    fn into_maps(mut self) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<RawRange>>) {
        self.values.remove(SECONDARY_CAPTURE);
        self.ranges.remove(SECONDARY_CAPTURE);
        (self.values, self.ranges)
    }
}

struct CompiledPattern {
    language: Language,
    expando: Expando,
    source: String,
    tree: Option<Tree>,
    special: Option<SpecialPattern>,
    candidate_plan: CandidatePlan,
}

enum SpecialPattern {
    HtmlTagName {
        capture: String,
    },
    KeyValuePair {
        key_capture: String,
        value_capture: String,
    },
}

impl CompiledPattern {
    fn new(lang: &AgLanguage, pattern: &str) -> Result<Self, String> {
        if let Some(capture) = html_tag_name_capture(pattern) {
            return Ok(Self {
                language: lang.tree_sitter_language(),
                expando: lang.expando(),
                source: pattern.to_owned(),
                tree: None,
                special: Some(SpecialPattern::HtmlTagName { capture }),
                candidate_plan: CandidatePlan::from_kinds([
                    "start_tag".to_owned(),
                    "self_closing_tag".to_owned(),
                ]),
            });
        }

        if let Some((key_capture, value_capture)) = key_value_pair_capture(pattern) {
            return Ok(Self {
                language: lang.tree_sitter_language(),
                expando: lang.expando(),
                source: pattern.to_owned(),
                tree: None,
                special: Some(SpecialPattern::KeyValuePair {
                    key_capture,
                    value_capture,
                }),
                candidate_plan: CandidatePlan::from_kinds([
                    "block_mapping_pair".to_owned(),
                    "pair".to_owned(),
                ]),
            });
        }

        if pattern.len() > 64_000 {
            return Err("structural pattern exceeds 64000 byte limit".to_owned());
        }
        let mut source = lang.preprocess_pattern(pattern).into_owned();
        let language = lang.tree_sitter_language();
        let mut tree = parse_tree(&language, &source).map_err(|err| err.to_string())?;
        // Parse fragments once, at compilation, for both direct patterns and
        // every nested YAML pattern. A grammar-checked terminator supplies
        // statement/declaration context without depending on source matches.
        // Complete constructs and unrelated shapes retain their original tree.
        if let Some(kind) = lang.terminated_fragment_kind() {
            if !source.trim_end().ends_with([';', '}']) {
                let contextual_source = format!("{source};");
                let contextual_tree =
                    parse_tree(&language, &contextual_source).map_err(|err| err.to_string())?;
                let contextual_root =
                    effective_pattern_root(contextual_tree.root_node(), &contextual_source);
                if contextual_root.kind() == kind && !contextual_root.has_error() {
                    source = contextual_source;
                    tree = contextual_tree;
                }
            }
        }
        if let Some(offset) = ambiguous_function_body_capture(
            effective_pattern_root(tree.root_node(), &source),
            &source,
            lang.expando(),
        ) {
            let mut contextual_source = source.clone();
            contextual_source.insert(offset, ';');
            let contextual_tree =
                parse_tree(&language, &contextual_source).map_err(|err| err.to_string())?;
            if effective_pattern_root(contextual_tree.root_node(), &contextual_source).kind()
                == "function_definition"
            {
                source = contextual_source;
                tree = contextual_tree;
            }
        }
        let root = effective_pattern_root(tree.root_node(), &source);
        if root.is_error() {
            return Err(
                "invalid structural pattern: pattern parsed with syntax errors".to_string(),
            );
        }
        let candidate_plan = if meta_from_node(root, &source, lang.expando()).is_some() {
            CandidatePlan::Any
        } else {
            CandidatePlan::from_kind(root.kind())
        };
        Ok(Self {
            language,
            expando: lang.expando(),
            source,
            tree: Some(tree),
            special: None,
            candidate_plan,
        })
    }

    fn language(&self) -> &Language {
        &self.language
    }

    fn is_special(&self) -> bool {
        self.special.is_some()
    }

    fn candidate_plan(&self) -> &CandidatePlan {
        &self.candidate_plan
    }

    fn matches_candidate(&self, candidate: Node<'_>) -> bool {
        self.candidate_plan.matches(candidate)
    }

    fn find_special_matches(
        &self,
        content: &str,
        deadline: Instant,
    ) -> Result<Vec<MatchWithKind>, ExecutionError> {
        let Some(special) = &self.special else {
            return Ok(Vec::new());
        };
        let tree = parse_tree_with_deadline(&self.language, content, deadline)?;

        let mut seen = HashSet::new();
        let mut matches = Vec::new();
        let line_index = LineIndex::new(content);
        visit_named(tree.root_node(), deadline, &mut |candidate| {
            if !self.matches_candidate(candidate) {
                return Ok(());
            }
            if let Some(matched) =
                self.special_structural_match(special, candidate, content, &line_index)
            {
                let key = (
                    matched.start_line,
                    matched.start_col,
                    matched.end_line,
                    matched.end_col,
                );
                if seen.insert(key) {
                    matches.push(MatchWithKind::new(candidate, matched));
                }
            }
            Ok(())
        })?;
        Ok(matches)
    }

    fn matches(
        &self,
        candidate: Node<'_>,
        content: &str,
        captures: &mut CaptureEnv,
        deadline: Instant,
    ) -> Result<bool, ExecutionError> {
        if let Some(special) = &self.special {
            return Ok(self.matches_special(special, candidate, content, captures));
        }

        let Some(tree) = &self.tree else {
            return Ok(false);
        };
        let root = effective_pattern_root(tree.root_node(), &self.source);
        let mut budget = MatchBudget {
            attempts: MAX_MULTI_CAPTURE_ATTEMPTS,
            deadline,
        };
        self.match_node(
            root,
            &self.source,
            candidate,
            content,
            captures,
            0,
            &mut budget,
        )
    }

    fn special_structural_match(
        &self,
        special: &SpecialPattern,
        candidate: Node<'_>,
        content: &str,
        line_index: &LineIndex,
    ) -> Option<StructuralMatch> {
        // Direct patterns and pattern rules share capture equality semantics.
        let mut captures = CaptureEnv::default();
        if !self.matches_special(special, candidate, content, &mut captures) {
            return None;
        }
        let (metavars, metavar_ranges_raw) = captures.into_maps();
        match special {
            SpecialPattern::HtmlTagName { .. } => {
                // Opening tags are shared by ordinary, script and style elements.
                // Matching this node also keeps raw_text contents out of the results.
                let opening_tag = candidate;
                Some(structural_match_from_byte_range_with_index(
                    content,
                    line_index,
                    opening_tag.start_byte(),
                    opening_tag.end_byte(),
                    metavars,
                    metavar_ranges_raw,
                ))
            }
            SpecialPattern::KeyValuePair { .. } => Some(to_structural_match_with_index(
                candidate,
                content,
                line_index,
                metavars,
                metavar_ranges_raw,
            )),
        }
    }

    fn matches_special(
        &self,
        special: &SpecialPattern,
        candidate: Node<'_>,
        content: &str,
        captures: &mut CaptureEnv,
    ) -> bool {
        match special {
            SpecialPattern::HtmlTagName { capture } => {
                let Some(tag_name) = html_tag_name_node(candidate) else {
                    return false;
                };
                captures.capture_one(
                    capture,
                    node_text(tag_name, content).to_owned(),
                    raw_range(tag_name),
                )
            }
            SpecialPattern::KeyValuePair {
                key_capture,
                value_capture,
            } => {
                let Some((key, value)) = key_value_nodes(candidate) else {
                    return false;
                };
                captures.capture_one(
                    key_capture,
                    node_text(key, content).to_owned(),
                    raw_range(key),
                ) && captures.capture_one(
                    value_capture,
                    node_text(value, content).to_owned(),
                    raw_range(value),
                )
            }
        }
    }

    // `depth` guards native-stack growth against pathologically nested patterns
    // (see `MAX_STRUCTURAL_DEPTH`); `attempts` is the shared `$$$` split budget
    // (see `MAX_MULTI_CAPTURE_ATTEMPTS`). The budget also carries the run deadline.
    fn match_node(
        &self,
        pattern: Node<'_>,
        pattern_source: &str,
        candidate: Node<'_>,
        candidate_source: &str,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        ExecutionError::check(budget.deadline)?;
        if depth >= MAX_STRUCTURAL_DEPTH {
            return Err(ExecutionError::limit(
                "structural.match.depthLimit",
                "match",
                "Structural pattern matching exceeded its recursion limit",
            ));
        }
        if let Some(meta) = meta_from_node(pattern, pattern_source, self.expando) {
            // A MISSING node is tree-sitter's zero-width error-recovery
            // placeholder for a token the grammar expected but the source
            // never had. A bare metavar (`$X`/`$_`) would otherwise bind to
            // it unconditionally, reporting a phantom match with empty
            // captured text on a syntactically broken file. This does not
            // exclude `is_error()` subtrees generally — those wrap real
            // (if malformed) source text and a legitimate match can still
            // occur inside them.
            if candidate.is_missing() {
                return Ok(false);
            }
            return Ok(match meta {
                MetaVar::Single(name) => captures.capture_one(
                    &name,
                    node_text(candidate, candidate_source).to_owned(),
                    raw_range(candidate),
                ),
                MetaVar::IgnoredSingle => true,
                MetaVar::Multi(_) | MetaVar::IgnoredMulti => false,
            });
        }

        if pattern.kind() != candidate.kind() {
            return Ok(false);
        }

        // Drop MISSING nodes from the PATTERN's own children before comparing
        // shape. A MISSING node is tree-sitter's zero-width error-recovery
        // stand-in for a token the grammar expected but never got — it can
        // never represent user intent (nobody types a "missing token" into a
        // pattern string). It shows up here specifically because some
        // grammars parse a bare `$$$NAME`/`$X` expando identifier at
        // statement position ambiguously (e.g. C/C++/C# treat an
        // unrecognized identifier as the start of a declaration and then
        // expect a trailing `;`) — the compiled pattern's root itself is not
        // `is_error()` (so `CompiledPattern::new` accepts it), but the
        // MISSING sibling it left behind can never match any real candidate
        // child, which silently broke every `{ $$$BODY }`-shaped pattern for
        // those grammars. The candidate side is deliberately left as-is: a
        // MISSING token in the real document being searched is genuine
        // evidence of broken source and must still fail to match.
        let pattern_children: Vec<Node<'_>> = children(pattern)
            .into_iter()
            .filter(|node| !node.is_missing())
            .collect();
        let candidate_children = children(candidate);
        if pattern_children.is_empty() && candidate_children.is_empty() {
            return Ok(node_text(pattern, pattern_source) == node_text(candidate, candidate_source));
        }

        self.match_child_list(
            &pattern_children,
            pattern_source,
            &candidate_children,
            candidate_source,
            captures,
            depth,
            budget,
        )
    }

    fn match_child_list(
        &self,
        mut pattern_children: &[Node<'_>],
        pattern_source: &str,
        mut candidate_children: &[Node<'_>],
        candidate_source: &str,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        // Ordinary siblings consume no native stack; only nested patterns and
        // multi-capture branches recurse, and both spend the depth guard.
        ExecutionError::check(budget.deadline)?;
        if depth >= MAX_STRUCTURAL_DEPTH {
            return Err(ExecutionError::limit(
                "structural.match.depthLimit",
                "match",
                "Structural pattern matching exceeded its recursion limit",
            ));
        }
        let mut branch = captures.clone();
        loop {
            let Some(first) = pattern_children.first().copied() else {
                if candidate_children.is_empty() {
                    *captures = branch;
                    return Ok(true);
                }
                return Ok(false);
            };
            let multi = match meta_from_node(first, pattern_source, self.expando) {
                Some(MetaVar::Multi(name)) => Some(name),
                Some(MetaVar::IgnoredMulti) => Some(None),
                _ => None,
            };
            if let Some(name) = multi {
                if self.match_multi_capture(
                    name.as_deref(),
                    &pattern_children[1..],
                    pattern_source,
                    candidate_children,
                    candidate_source,
                    &mut branch,
                    depth + 1,
                    budget,
                )? {
                    *captures = branch;
                    return Ok(true);
                }
                return Ok(false);
            }
            let Some(candidate_first) = candidate_children.first().copied() else {
                return Ok(false);
            };
            if !self.match_node(
                first,
                pattern_source,
                candidate_first,
                candidate_source,
                &mut branch,
                depth + 1,
                budget,
            )? {
                return Ok(false);
            }
            pattern_children = &pattern_children[1..];
            candidate_children = &candidate_children[1..];
        }
    }

    fn match_multi_capture(
        &self,
        name: Option<&str>,
        remaining_pattern: &[Node<'_>],
        pattern_source: &str,
        candidate_children: &[Node<'_>],
        candidate_source: &str,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        let min_remaining =
            minimum_candidate_nodes(remaining_pattern, pattern_source, self.expando);
        if candidate_children.len() < min_remaining {
            return Ok(false);
        }
        let max_take = candidate_children.len() - min_remaining;
        for take in 0..=max_take {
            ExecutionError::check(budget.deadline)?;
            // Each split point is one unit of the shared backtracking budget;
            // exhausting it bails the whole match rather than continuing to
            // explore a combinatorial split space.
            if budget.attempts == 0 {
                return Err(ExecutionError::limit(
                    "structural.match.backtrackingLimit",
                    "match",
                    "Structural matching exhausted its split-attempt budget",
                ));
            }
            budget.attempts -= 1;
            let mut branch = captures.clone();
            if let Some(name) = name {
                let texts = candidate_children[..take]
                    .iter()
                    .map(|node| node_text(*node, candidate_source).to_owned())
                    .collect();
                let ranges = candidate_children[..take]
                    .iter()
                    .map(|node| raw_range(*node))
                    .collect();
                if !branch.capture_many(name, texts, ranges) {
                    continue;
                }
            }
            if self.match_child_list(
                remaining_pattern,
                pattern_source,
                &candidate_children[take..],
                candidate_source,
                &mut branch,
                depth,
                budget,
            )? {
                *captures = branch;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn html_tag_name_node(candidate: Node<'_>) -> Option<Node<'_>> {
    if !matches!(candidate.kind(), "start_tag" | "self_closing_tag") {
        return None;
    }
    named_children(candidate)
        .into_iter()
        .find(|node| node.kind() == "tag_name")
}

fn key_value_nodes(candidate: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    if !matches!(candidate.kind(), "pair" | "block_mapping_pair") {
        return None;
    }
    let named = named_children(candidate);
    match named.as_slice() {
        [key, value, ..] => Some((*key, *value)),
        _ => None,
    }
}

fn html_tag_name_capture(pattern: &str) -> Option<String> {
    let trimmed = pattern.trim();
    let inner = trimmed.strip_prefix("<$")?.strip_suffix('>')?;
    if is_capture_name(inner) {
        return Some(inner.to_owned());
    }
    None
}

fn key_value_pair_capture(pattern: &str) -> Option<(String, String)> {
    let (left, right) = pattern.trim().split_once(':')?;
    let key_capture = capture_name_from_token(left.trim())?;
    let value_capture = capture_name_from_token(right.trim())?;
    Some((key_capture, value_capture))
}

fn capture_name_from_token(token: &str) -> Option<String> {
    let name = token.strip_prefix('$')?;
    if is_capture_name(name) {
        return Some(name.to_owned());
    }
    None
}

fn minimum_candidate_nodes(pattern_children: &[Node<'_>], source: &str, expando: Expando) -> usize {
    pattern_children
        .iter()
        .filter(|node| {
            !matches!(
                meta_from_node(**node, source, expando),
                Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
            )
        })
        .count()
}

#[derive(Debug, PartialEq, Eq)]
enum MetaVar {
    Single(String),
    Multi(Option<String>),
    IgnoredSingle,
    IgnoredMulti,
}

/// C++ can parse `{ $$$BODY }` after a function declarator as an initializer
/// list. A statement terminator disambiguates only this sole-capture body;
/// ordinary variable initializers and concrete expression lists stay untouched.
fn ambiguous_function_body_capture(
    root: Node<'_>,
    source: &str,
    expando: Expando,
) -> Option<usize> {
    if root.kind() != "declaration" {
        return None;
    }
    let initializer = root.child_by_field_name("declarator")?;
    if initializer.kind() != "init_declarator"
        || initializer.child_by_field_name("declarator")?.kind() != "function_declarator"
    {
        return None;
    }
    let body = initializer.child_by_field_name("value")?;
    if body.kind() != "initializer_list" {
        return None;
    }
    let named = named_children(body);
    let [capture] = named.as_slice() else {
        return None;
    };
    matches!(
        meta_from_node(*capture, source, expando),
        Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
    )
    .then_some(capture.end_byte())
}

/// `expando.primary` and `expando.bare_word` differ only for PHP (see
/// `Expando`'s doc comment) — a metavar substituted at a bare-word position
/// (a function name) used `bare_word`, everything else used `primary`. Both
/// must be recognized here: the leading char run is checked against
/// *either*, and the repeat-count is taken against whichever one it actually
/// is (never a mix of the two).
fn meta_from_node(node: Node<'_>, source: &str, expando: Expando) -> Option<MetaVar> {
    if node.kind() == "expression_statement" {
        let named = named_children(node);
        if let [capture] = named.as_slice() {
            let meta = meta_from_text(node_text(*capture, source), expando);
            if matches!(meta, Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)) {
                return meta;
            }
        }
    }
    meta_from_text(node_text(node, source), expando)
}

fn meta_from_text(text: &str, expando: Expando) -> Option<MetaVar> {
    let mut chars = text.chars();
    let leading = chars.next()?;
    if !expando.matches_leading(leading) {
        return None;
    }

    let expando_len = text.chars().take_while(|ch| *ch == leading).count();
    let rest: String = text.chars().skip(expando_len).collect();
    match expando_len {
        1 if rest == "_" => Some(MetaVar::IgnoredSingle),
        1 if is_capture_name(&rest) => Some(MetaVar::Single(rest)),
        3 if rest.is_empty() => Some(MetaVar::IgnoredMulti),
        3 if is_capture_name(&rest) => Some(MetaVar::Multi(Some(rest))),
        _ => None,
    }
}

fn is_capture_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRuleDocument {
    rule: RawRule,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRule {
    kind: Option<String>,
    pub(super) pattern: Option<String>,
    regex: Option<String>,
    pub(super) has: Option<Box<RawRule>>,
    pub(super) inside: Option<Box<RawRule>>,
    pub(super) all: Option<Vec<RawRule>>,
    pub(super) any: Option<Vec<RawRule>>,
    not: Option<Box<RawRule>>,
    #[serde(rename = "stopBy")]
    stop_by: Option<RawStopBy>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum RawStopBy {
    End,
}

struct CompiledRule {
    kind: Option<String>,
    pattern: Option<CompiledPattern>,
    regex: Option<Regex>,
    has: Option<Box<CompiledRule>>,
    inside: Option<Box<CompiledRule>>,
    all: Vec<CompiledRule>,
    any: Vec<CompiledRule>,
    not: Option<Box<CompiledRule>>,
    stop_by_end: bool,
    candidate_plan: CandidatePlan,
}

pub(super) fn parse_rule(rule: &str) -> Result<RawRule, String> {
    #[cfg(test)]
    RULE_PARSE_COUNT.with(|count| count.set(count.get() + 1));
    if rule.len() > 64_000 {
        return Err("structural rule exceeds 64000 byte limit".to_owned());
    }
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(rule).map_err(|err| format!("invalid rule YAML: {err}"))?;
    let wrapped = value
        .as_mapping()
        .is_some_and(|mapping| mapping.contains_key("rule"));
    let raw: RawRule = if wrapped {
        serde_yaml_ng::from_value::<RawRuleDocument>(value)
            .map_err(|err| format!("invalid rule YAML: {err}"))?
            .rule
    } else {
        serde_yaml_ng::from_value(value).map_err(|err| format!("invalid rule YAML: {err}"))?
    };
    validate_rule_depth(&raw, 0)?;
    Ok(raw)
}

#[cfg(test)]
thread_local! {
    static RULE_PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn validate_rule_depth(rule: &RawRule, depth: usize) -> Result<(), String> {
    if depth >= 64 {
        return Err("structural rule exceeds 64 nesting levels".to_owned());
    }
    for child in rule
        .all
        .iter()
        .flatten()
        .chain(rule.any.iter().flatten())
        .chain(rule.has.iter().map(Box::as_ref))
        .chain(rule.inside.iter().map(Box::as_ref))
        .chain(rule.not.iter().map(Box::as_ref))
    {
        validate_rule_depth(child, depth + 1)?;
    }
    Ok(())
}

impl CompiledRule {
    /// Accepts both the wrapped document form (`rule:\n  kind: ...`) and a bare
    /// rule (`kind: ...`). A top-level `rule` key is unambiguous: `RawRule` has
    /// no such field, so a bare rule can never contain one.
    #[cfg(test)]
    fn new(lang: &AgLanguage, rule: &str) -> Result<Self, String> {
        let raw = parse_rule(rule)?;
        Self::compile(lang, &raw)
    }

    fn compile(lang: &AgLanguage, raw: &RawRule) -> Result<Self, String> {
        if let Some(kind) = raw.kind.as_deref() {
            let language = lang.tree_sitter_language();
            // ERROR is a built-in recovery node, outside the grammar's symbol table.
            if kind != "ERROR"
                && !(0..language.node_kind_count())
                    .any(|id| language.node_kind_for_id(id as u16) == Some(kind))
            {
                return Err(format!(
                    "unknown node kind '{kind}' for this language grammar"
                ));
            }
        }
        let pattern = raw
            .pattern
            .as_deref()
            .map(|pattern| CompiledPattern::new(lang, pattern))
            .transpose()?;
        let regex = raw
            .regex
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|err| format!("invalid rule regex: {err}"))?;
        let has = raw
            .has
            .as_deref()
            .map(|rule| Self::compile(lang, rule).map(Box::new))
            .transpose()?;
        let inside = raw
            .inside
            .as_deref()
            .map(|rule| Self::compile(lang, rule).map(Box::new))
            .transpose()?;
        let all = raw
            .all
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|rule| Self::compile(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let any = raw
            .any
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|rule| Self::compile(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let not = raw
            .not
            .as_deref()
            .map(|rule| Self::compile(lang, rule).map(Box::new))
            .transpose()?;

        let mut compiled = Self {
            kind: raw.kind.clone(),
            pattern,
            regex,
            has,
            inside,
            all,
            any,
            not,
            stop_by_end: raw.stop_by == Some(RawStopBy::End),
            candidate_plan: CandidatePlan::Any,
        };
        if compiled.is_empty() {
            return Err("invalid rule: rule must contain at least one matcher".to_string());
        }
        compiled.candidate_plan = compiled.compute_candidate_plan();
        Ok(compiled)
    }

    fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.pattern.is_none()
            && self.regex.is_none()
            && self.has.is_none()
            && self.inside.is_none()
            && self.all.is_empty()
            && self.any.is_empty()
            && self.not.is_none()
    }

    fn simple_kind(&self) -> Option<&str> {
        let kind = self.kind.as_deref()?;
        (self.pattern.is_none()
            && self.regex.is_none()
            && self.has.is_none()
            && self.inside.is_none()
            && self.all.is_empty()
            && self.any.is_empty()
            && self.not.is_none())
        .then_some(kind)
    }

    fn compute_candidate_plan(&self) -> CandidatePlan {
        let mut plan = CandidatePlan::Any;
        if let Some(kind) = &self.kind {
            plan = plan.intersect(CandidatePlan::from_kind(kind.clone()));
        }
        if let Some(pattern) = &self.pattern {
            plan = plan.intersect(pattern.candidate_plan().clone());
        }
        for rule in &self.all {
            plan = plan.intersect(rule.candidate_plan.clone());
        }
        if !self.any.is_empty() {
            let any_plan =
                CandidatePlan::union(self.any.iter().map(|rule| rule.candidate_plan.clone()));
            plan = plan.intersect(any_plan);
        }
        plan
    }

    fn matches_candidate(&self, candidate: Node<'_>) -> bool {
        self.candidate_plan.matches(candidate)
    }

    fn matches(
        &self,
        candidate: Node<'_>,
        document: &Document<'_>,
        captures: &mut CaptureEnv,
    ) -> Result<bool, ExecutionError> {
        ExecutionError::check(document.deadline)?;
        if !self.matches_candidate(candidate) {
            return Ok(false);
        }
        if let Some(kind) = &self.kind {
            if candidate.kind() != kind {
                return Ok(false);
            }
        }
        if let Some(pattern) = &self.pattern {
            if !pattern.matches(candidate, document.content, captures, document.deadline)? {
                return Ok(false);
            }
        }
        if let Some(regex) = &self.regex {
            if !regex.is_match(node_text(candidate, document.content)) {
                return Ok(false);
            }
        }
        if let Some(rule) = &self.has {
            let mut branch = captures.clone();
            if !matches_descendant(rule, candidate, document, &mut branch, 0)? {
                return Ok(false);
            }
            *captures = branch;
        }
        if let Some(rule) = &self.inside {
            let mut branch = captures.clone();
            if !matches_ancestor(rule, candidate, document, &mut branch)? {
                return Ok(false);
            }
            *captures = branch;
        }
        for rule in &self.all {
            let mut branch = captures.clone();
            if !rule.matches(candidate, document, &mut branch)? {
                return Ok(false);
            }
            *captures = branch;
        }
        if !self.any.is_empty() {
            let mut matched = None;
            for rule in &self.any {
                let mut branch = captures.clone();
                if rule.matches(candidate, document, &mut branch)? {
                    matched = Some(branch);
                    break;
                }
            }
            let Some(branch) = matched else {
                return Ok(false);
            };
            *captures = branch;
        }
        if let Some(rule) = &self.not {
            let mut branch = captures.clone();
            if rule.matches(candidate, document, &mut branch)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

struct Document<'a> {
    content: &'a str,
    deadline: Instant,
}

fn matches_descendant(
    rule: &CompiledRule,
    candidate: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
    _depth: usize,
) -> Result<bool, ExecutionError> {
    let mut stack = named_children(candidate);
    stack.reverse();
    while let Some(child) = stack.pop() {
        ExecutionError::check(document.deadline)?;
        if rule.matches_candidate(child) {
            let mut branch = captures.clone();
            if rule.matches(child, document, &mut branch)? {
                branch.capture_replace(
                    SECONDARY_CAPTURE,
                    node_text(child, document.content).to_owned(),
                    raw_range(child),
                );
                *captures = branch;
                return Ok(true);
            }
        }
        if rule.stop_by_end {
            stack.extend(named_children(child).into_iter().rev());
        }
    }
    Ok(false)
}

fn matches_ancestor(
    rule: &CompiledRule,
    candidate: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
) -> Result<bool, ExecutionError> {
    let mut parent = candidate.parent();
    while let Some(node) = parent {
        ExecutionError::check(document.deadline)?;
        if rule.matches_candidate(node) {
            let mut branch = captures.clone();
            if rule.matches(node, document, &mut branch)? {
                branch.capture_replace(
                    SECONDARY_CAPTURE,
                    node_text(node, document.content).to_owned(),
                    raw_range(node),
                );
                *captures = branch;
                return Ok(true);
            }
        }
        if !rule.stop_by_end {
            return Ok(false);
        }
        parent = node.parent();
    }
    Ok(false)
}

/// Synthetic class name `preprocess_pattern` wraps every C# pattern in — see
/// `AgLanguage::class_wrap`. Real user patterns essentially never target a
/// class literally named this, so matching on it by name (rather than by
/// kind, like the other wrapper cases below) is safe: it only unwraps
/// *our own* synthetic wrapper, never a real `class $NAME { ... }` pattern
/// whose outer class_declaration the user actually wants to match.
const CSHARP_WRAP_MARKER: &str = "__OctoWrap";

fn effective_pattern_root<'a>(mut node: Node<'a>, source: &str) -> Node<'a> {
    loop {
        let named = named_children(node);
        if named.len() == 1 && is_pattern_wrapper(node.kind()) {
            node = named[0];
            continue;
        }
        // PHP's `program` wraps a leading `php_tag` (from the `<?php `
        // prefix `preprocess_pattern` adds) alongside the real content node
        // — two named children, so the single-child unwrap above doesn't
        // apply. Skip the tag and continue unwrapping from the real content.
        if named.len() == 2 && node.kind() == "program" && named[0].kind() == "php_tag" {
            node = named[1];
            continue;
        }
        // C#'s synthetic wrapper class (see CSHARP_WRAP_MARKER doc above):
        // unwrap `class __OctoWrap { <member> }` down to the single real
        // member, giving it real class-body context (a bare `public int
        // Foo(...) { ... }` parsed standalone isn't valid C# at all — no
        // top-level member/method syntax exists outside a type body).
        if node.kind() == "class_declaration" {
            let is_wrapper = node
                .child_by_field_name("name")
                .is_some_and(|n| node_text(n, source) == CSHARP_WRAP_MARKER);
            if is_wrapper {
                if let Some(body) = node.child_by_field_name("body") {
                    let body_named = named_children(body);
                    if body_named.len() == 1 {
                        node = body_named[0];
                        continue;
                    }
                }
            }
        }
        break node;
    }
}

fn is_pattern_wrapper(kind: &str) -> bool {
    matches!(
        kind,
        "program"
            | "source_file"
            | "module"
            | "compilation_unit"
            | "translation_unit"
            | "stylesheet"
            | "fragment"
            | "document"
            | "expression_statement"
            | "config_file"
            | "body"
    )
}

fn children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::with_capacity(node.child_count() as usize);
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            out.push(child);
        }
    }
    out
}

fn named_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::with_capacity(node.named_child_count());
    for index in 0..node.named_child_count() {
        if let Some(child) = node.named_child(index as u32) {
            out.push(child);
        }
    }
    out
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
}

/// Thin wrapper over the shared `text::utf8_offsets::LineIndex` — see that
/// type for the actual line-start/UTF-16 counting logic. Keeps this module's
/// 1-based-line, tree-sitter-point-shaped call sites unchanged.
struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    fn byte_to_line_col(&self, byte: usize) -> (usize, usize) {
        let (row, column) = self.0.byte_to_position(byte as u32);
        (row as usize + 1, column as usize)
    }

    /// Convert a tree-sitter byte column to an LSP-compatible **UTF-16 code-unit**
    /// column. This is the unit `lspGetSemantics` uses, the JS resolver emits
    /// (`resolver::byte_offset_to_utf16`), and the signatures layer reports
    /// (`char::len_utf16`). Counting Unicode scalar values (`chars().count()`)
    /// instead would disagree with every other layer on any line containing a
    /// non-BMP character (e.g. an emoji is one code point but two UTF-16 units).
    fn point_column_to_char_column(&self, row: usize, byte_column: usize) -> usize {
        self.0
            .row_col_to_utf16_column(row as u32, byte_column as u32) as usize
    }
}

/// Converts raw tree-sitter capture positions into `MetavarRange`s (1-based
/// line, char column), pairing each range with its captured text by index.
fn build_metavar_ranges(
    line_index: &LineIndex,
    values: &HashMap<String, Vec<String>>,
    raw: HashMap<String, Vec<RawRange>>,
) -> HashMap<String, Vec<MetavarRange>> {
    raw.into_iter()
        .map(|(name, ranges)| {
            let texts = values.get(&name);
            let mapped = ranges
                .into_iter()
                .enumerate()
                .map(|(i, (sr, sc, er, ec))| MetavarRange {
                    text: texts.and_then(|t| t.get(i)).cloned().unwrap_or_default(),
                    line: sr + 1,
                    column: line_index.point_column_to_char_column(sr as usize, sc as usize) as u32,
                    end_line: er + 1,
                    end_column: line_index.point_column_to_char_column(er as usize, ec as usize)
                        as u32,
                })
                .collect();
            (name, mapped)
        })
        .collect()
}

fn to_structural_match(
    node: Node<'_>,
    content: &str,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let line_index = LineIndex::new(content);
    to_structural_match_with_index(node, content, &line_index, metavars, metavar_ranges_raw)
}

fn to_structural_match_with_index(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let start = node.start_position();
    let end = node.end_position();
    let metavar_ranges = build_metavar_ranges(line_index, &metavars, metavar_ranges_raw);
    StructuralMatch {
        start_line: (start.row as u32) + 1,
        end_line: (end.row as u32) + 1,
        start_col: line_index.point_column_to_char_column(start.row, start.column) as u32,
        end_col: line_index.point_column_to_char_column(end.row, end.column) as u32,
        text: node_text(node, content).to_owned(),
        metavars,
        metavar_ranges,
    }
}

fn structural_match_from_byte_range_with_index(
    content: &str,
    line_index: &LineIndex,
    start_byte: usize,
    end_byte: usize,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let (start_line, start_col) = line_index.byte_to_line_col(start_byte);
    let (end_line, end_col) = line_index.byte_to_line_col(end_byte);
    let metavar_ranges = build_metavar_ranges(line_index, &metavars, metavar_ranges_raw);
    StructuralMatch {
        start_line: start_line as u32,
        end_line: end_line as u32,
        start_col: start_col as u32,
        end_col: end_col as u32,
        text: content
            .get(start_byte..end_byte)
            .unwrap_or_default()
            .to_owned(),
        metavars,
        metavar_ranges,
    }
}

#[cfg(test)]
#[path = "octo_tests.rs"]
mod tests;
