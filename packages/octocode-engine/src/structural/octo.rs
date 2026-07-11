use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde::Deserialize;
use tree_sitter::{Language, Node, Parser, Tree};

use super::language::AgLanguage;
use super::query::StructuralQuery;
use super::types::{MetavarRange, StructuralMatch};

pub(super) type OctoCompiledMatcher = Box<dyn Fn(&str) -> Vec<MatchWithKind> + Send + Sync>;

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

/// Max nesting the recursive tree walkers and pattern matcher descend before
/// giving up. Named AST nodes almost never nest anywhere near this in real
/// source; the cap exists only so a pathological input (e.g. a ~1 MB string of
/// `[[[[…`) can't overflow the native stack. `structuralSearch` is a *sync*
/// napi call, and a stack overflow raises SIGSEGV which `catch_unwind` cannot
/// catch — it aborts the whole Node process. Anything nested deeper than this
/// is simply not visited / not matched (a bounded, graceful loss of recall).
const MAX_STRUCTURAL_DEPTH: usize = 500;

/// Budget on `$$$` multi-metavar split attempts within a single top-level
/// child-list match. Each `$$$` tries every split point (`for take in
/// 0..=max_take`) and several `$$$` against a wide node are combinatorial; this
/// caps the total work so a crafted pattern + wide input can't stall for
/// seconds. When the budget is exhausted the match bails to no-match.
const MAX_MULTI_CAPTURE_ATTEMPTS: usize = 10_000;

pub(super) fn compile_matcher(
    lang: &AgLanguage,
    query: StructuralQuery<'_>,
) -> Result<OctoCompiledMatcher, String> {
    let language = lang.tree_sitter_language();
    match query.parts() {
        (Some(pattern), None) if is_document_probe(pattern) => Ok(Box::new(move |content| {
            parse_tree(&language, content)
                .map(|tree| {
                    let root = tree.root_node();
                    vec![MatchWithKind::new(
                        root,
                        to_structural_match(root, content, HashMap::new(), HashMap::new()),
                    )]
                })
                .unwrap_or_default()
        })),
        (Some(pattern), None) => {
            let compiled = CompiledPattern::new(lang, pattern)?;
            Ok(Box::new(move |content| {
                if compiled.is_special() {
                    return compiled.find_special_matches(content);
                }

                let Some(tree) = parse_tree(compiled.language(), content) else {
                    return Vec::new();
                };
                let line_index = LineIndex::new(content);
                let mut matches = Vec::new();
                visit_named(tree.root_node(), 0, &mut |candidate| {
                    if !compiled.matches_candidate(candidate) {
                        return;
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, content, &mut captures) {
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
                });
                matches
            }))
        }
        (None, Some(rule)) => {
            let compiled = CompiledRule::new(lang, rule)?;
            let language = lang.tree_sitter_language();
            if let Some(kind) = compiled.simple_kind().map(str::to_owned) {
                return Ok(Box::new(move |content| {
                    let Some(tree) = parse_tree(&language, content) else {
                        return Vec::new();
                    };
                    collect_kind_matches(tree.root_node(), &kind, content)
                }));
            }
            Ok(Box::new(move |content| {
                let Some(tree) = parse_tree(&language, content) else {
                    return Vec::new();
                };
                let document = Document { content };
                let line_index = LineIndex::new(content);
                let mut matches = Vec::new();
                visit_named(tree.root_node(), 0, &mut |candidate| {
                    if !compiled.matches_candidate(candidate) {
                        return;
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, &document, &mut captures) {
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
                });
                matches
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
    /// `rayon` pool. Each rayon worker gets its own instance (`Parser` is not
    /// `Sync`), and `set_language` is re-applied per parse: a single-extension
    /// group already shares one grammar, so the call is cheap relative to
    /// constructing a parser from scratch.
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

fn parse_tree(language: &Language, content: &str) -> Option<Tree> {
    PARSER.with(|parser| {
        let mut parser = parser.borrow_mut();
        parser.set_language(language).ok()?;
        parser.parse(content.as_bytes(), None)
    })
}

fn visit_named<'tree>(node: Node<'tree>, depth: usize, f: &mut impl FnMut(Node<'tree>)) {
    if node.is_named() {
        f(node);
    }
    if depth >= MAX_STRUCTURAL_DEPTH {
        return;
    }
    for index in 0..node.named_child_count() {
        if let Some(child) = node.named_child(index as u32) {
            visit_named(child, depth + 1, f);
        }
    }
}

fn collect_kind_matches(root: Node<'_>, kind: &str, content: &str) -> Vec<MatchWithKind> {
    let line_index = LineIndex::new(content);
    let mut matches = Vec::new();
    visit_named(root, 0, &mut |candidate| {
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
    });
    matches
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
    expando: char,
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
                expando: lang.expando_char_value(),
                source: pattern.to_owned(),
                tree: None,
                special: Some(SpecialPattern::HtmlTagName { capture }),
                candidate_plan: CandidatePlan::from_kinds([
                    "element".to_owned(),
                    "self_closing_tag".to_owned(),
                ]),
            });
        }

        if let Some((key_capture, value_capture)) = key_value_pair_capture(pattern) {
            return Ok(Self {
                language: lang.tree_sitter_language(),
                expando: lang.expando_char_value(),
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

        let source = lang.preprocess_pattern(pattern).into_owned();
        let language = lang.tree_sitter_language();
        let tree = parse_tree(&language, &source)
            .ok_or_else(|| "invalid structural pattern: failed to parse pattern".to_string())?;
        let root = effective_pattern_root(tree.root_node());
        if root.is_error() {
            return Err(
                "invalid structural pattern: pattern parsed with syntax errors".to_string(),
            );
        }
        let candidate_plan = if meta_from_node(root, &source, lang.expando_char_value()).is_some() {
            CandidatePlan::Any
        } else {
            CandidatePlan::from_kind(root.kind())
        };
        Ok(Self {
            language,
            expando: lang.expando_char_value(),
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

    fn find_special_matches(&self, content: &str) -> Vec<MatchWithKind> {
        let Some(special) = &self.special else {
            return Vec::new();
        };
        let Some(tree) = parse_tree(&self.language, content) else {
            return Vec::new();
        };

        let mut seen = HashSet::new();
        let mut matches = Vec::new();
        let line_index = LineIndex::new(content);
        visit_named(tree.root_node(), 0, &mut |candidate| {
            if !self.matches_candidate(candidate) {
                return;
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
        });
        matches
    }

    fn matches(&self, candidate: Node<'_>, content: &str, captures: &mut CaptureEnv) -> bool {
        if let Some(special) = &self.special {
            return self.matches_special(special, candidate, content, captures);
        }

        let Some(tree) = &self.tree else {
            return false;
        };
        let root = effective_pattern_root(tree.root_node());
        let mut attempts = MAX_MULTI_CAPTURE_ATTEMPTS;
        self.match_node(
            root,
            &self.source,
            candidate,
            content,
            captures,
            0,
            &mut attempts,
        )
    }

    fn special_structural_match(
        &self,
        special: &SpecialPattern,
        candidate: Node<'_>,
        content: &str,
        line_index: &LineIndex,
    ) -> Option<StructuralMatch> {
        match special {
            SpecialPattern::HtmlTagName { capture } => {
                let tag_name = html_tag_name_node(candidate)?;
                let text = node_text(candidate, content);
                let open_tag_len = text.find('>')? + 1;
                let start_byte = candidate.start_byte();
                let end_byte = start_byte + open_tag_len;
                let mut metavars = HashMap::new();
                metavars.insert(
                    capture.clone(),
                    vec![node_text(tag_name, content).to_owned()],
                );
                let mut metavar_ranges_raw = HashMap::new();
                metavar_ranges_raw.insert(capture.clone(), vec![raw_range(tag_name)]);
                Some(structural_match_from_byte_range_with_index(
                    content,
                    line_index,
                    start_byte,
                    end_byte,
                    metavars,
                    metavar_ranges_raw,
                ))
            }
            SpecialPattern::KeyValuePair {
                key_capture,
                value_capture,
            } => {
                let (key, value) = key_value_nodes(candidate)?;
                let mut metavars = HashMap::new();
                metavars.insert(
                    key_capture.clone(),
                    vec![node_text(key, content).to_owned()],
                );
                metavars.insert(
                    value_capture.clone(),
                    vec![node_text(value, content).to_owned()],
                );
                let mut metavar_ranges_raw = HashMap::new();
                metavar_ranges_raw.insert(key_capture.clone(), vec![raw_range(key)]);
                metavar_ranges_raw.insert(value_capture.clone(), vec![raw_range(value)]);
                Some(to_structural_match_with_index(
                    candidate,
                    content,
                    line_index,
                    metavars,
                    metavar_ranges_raw,
                ))
            }
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
    // (see `MAX_MULTI_CAPTURE_ATTEMPTS`). Threading both explicitly keeps the
    // matcher a plain set of methods rather than a stateful struct.
    fn match_node(
        &self,
        pattern: Node<'_>,
        pattern_source: &str,
        candidate: Node<'_>,
        candidate_source: &str,
        captures: &mut CaptureEnv,
        depth: usize,
        attempts: &mut usize,
    ) -> bool {
        if depth >= MAX_STRUCTURAL_DEPTH {
            return false;
        }
        if let Some(meta) = meta_from_node(pattern, pattern_source, self.expando) {
            return match meta {
                MetaVar::Single(name) => captures.capture_one(
                    &name,
                    node_text(candidate, candidate_source).to_owned(),
                    raw_range(candidate),
                ),
                MetaVar::IgnoredSingle => true,
                MetaVar::Multi(_) | MetaVar::IgnoredMulti => false,
            };
        }

        if pattern.kind() != candidate.kind() {
            return false;
        }

        let pattern_children = children(pattern);
        let candidate_children = children(candidate);
        if pattern_children.is_empty() && candidate_children.is_empty() {
            return node_text(pattern, pattern_source) == node_text(candidate, candidate_source);
        }

        self.match_child_list(
            &pattern_children,
            pattern_source,
            &candidate_children,
            candidate_source,
            captures,
            depth,
            attempts,
        )
    }

    fn match_child_list(
        &self,
        pattern_children: &[Node<'_>],
        pattern_source: &str,
        candidate_children: &[Node<'_>],
        candidate_source: &str,
        captures: &mut CaptureEnv,
        depth: usize,
        attempts: &mut usize,
    ) -> bool {
        if pattern_children.is_empty() {
            return candidate_children.is_empty();
        }

        let first = pattern_children[0];
        if let Some(meta) = meta_from_node(first, pattern_source, self.expando) {
            match meta {
                MetaVar::Multi(name) => {
                    return self.match_multi_capture(
                        name.as_deref(),
                        &pattern_children[1..],
                        pattern_source,
                        candidate_children,
                        candidate_source,
                        captures,
                        depth,
                        attempts,
                    );
                }
                MetaVar::IgnoredMulti => {
                    return self.match_multi_capture(
                        None,
                        &pattern_children[1..],
                        pattern_source,
                        candidate_children,
                        candidate_source,
                        captures,
                        depth,
                        attempts,
                    );
                }
                MetaVar::Single(_) | MetaVar::IgnoredSingle => {}
            }
        }

        let Some(candidate_first) = candidate_children.first().copied() else {
            return false;
        };
        let mut branch = captures.clone();
        // Descending into a child is one level deeper; sibling recursion below
        // stays at the same depth.
        if !self.match_node(
            first,
            pattern_source,
            candidate_first,
            candidate_source,
            &mut branch,
            depth + 1,
            attempts,
        ) {
            return false;
        }
        if self.match_child_list(
            &pattern_children[1..],
            pattern_source,
            &candidate_children[1..],
            candidate_source,
            &mut branch,
            depth,
            attempts,
        ) {
            *captures = branch;
            return true;
        }
        false
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
        attempts: &mut usize,
    ) -> bool {
        let min_remaining =
            minimum_candidate_nodes(remaining_pattern, pattern_source, self.expando);
        if candidate_children.len() < min_remaining {
            return false;
        }
        let max_take = candidate_children.len() - min_remaining;
        for take in 0..=max_take {
            // Each split point is one unit of the shared backtracking budget;
            // exhausting it bails the whole match rather than continuing to
            // explore a combinatorial split space.
            if *attempts == 0 {
                return false;
            }
            *attempts -= 1;
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
                attempts,
            ) {
                *captures = branch;
                return true;
            }
        }
        false
    }
}

fn html_tag_name_node(candidate: Node<'_>) -> Option<Node<'_>> {
    if !matches!(candidate.kind(), "element" | "self_closing_tag") {
        return None;
    }
    first_named_descendant_kind(candidate, "tag_name", 0)
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

fn minimum_candidate_nodes(pattern_children: &[Node<'_>], source: &str, expando: char) -> usize {
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

fn meta_from_node(node: Node<'_>, source: &str, expando: char) -> Option<MetaVar> {
    let text = node_text(node, source);
    let mut chars = text.chars();
    if chars.next()? != expando {
        return None;
    }

    let expando_len = text.chars().take_while(|ch| *ch == expando).count();
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
struct RawRule {
    kind: Option<String>,
    pattern: Option<String>,
    regex: Option<String>,
    has: Option<Box<RawRule>>,
    inside: Option<Box<RawRule>>,
    all: Option<Vec<RawRule>>,
    any: Option<Vec<RawRule>>,
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

impl CompiledRule {
    /// Accepts both the wrapped document form (`rule:\n  kind: ...`) and a bare
    /// rule (`kind: ...`). A top-level `rule` key is unambiguous: `RawRule` has
    /// no such field, so a bare rule can never contain one.
    fn new(lang: &AgLanguage, rule: &str) -> Result<Self, String> {
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
        Self::compile(lang, raw)
    }

    fn compile(lang: &AgLanguage, raw: RawRule) -> Result<Self, String> {
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
            .map(|rule| Self::compile(lang, *rule).map(Box::new))
            .transpose()?;
        let inside = raw
            .inside
            .map(|rule| Self::compile(lang, *rule).map(Box::new))
            .transpose()?;
        let all = raw
            .all
            .unwrap_or_default()
            .into_iter()
            .map(|rule| Self::compile(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let any = raw
            .any
            .unwrap_or_default()
            .into_iter()
            .map(|rule| Self::compile(lang, rule))
            .collect::<Result<Vec<_>, _>>()?;
        let not = raw
            .not
            .map(|rule| Self::compile(lang, *rule).map(Box::new))
            .transpose()?;

        let mut compiled = Self {
            kind: raw.kind,
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
    ) -> bool {
        if !self.matches_candidate(candidate) {
            return false;
        }
        if let Some(kind) = &self.kind {
            if candidate.kind() != kind {
                return false;
            }
        }
        if let Some(pattern) = &self.pattern {
            if !pattern.matches(candidate, document.content, captures) {
                return false;
            }
        }
        if let Some(regex) = &self.regex {
            if !regex.is_match(node_text(candidate, document.content)) {
                return false;
            }
        }
        if let Some(rule) = &self.has {
            let mut branch = captures.clone();
            if !matches_descendant(rule, candidate, document, &mut branch, 0) {
                return false;
            }
            *captures = branch;
        }
        if let Some(rule) = &self.inside {
            let mut branch = captures.clone();
            if !matches_ancestor(rule, candidate, document, &mut branch) {
                return false;
            }
            *captures = branch;
        }
        for rule in &self.all {
            let mut branch = captures.clone();
            if !rule.matches(candidate, document, &mut branch) {
                return false;
            }
            *captures = branch;
        }
        if !self.any.is_empty() {
            let mut matched = None;
            for rule in &self.any {
                let mut branch = captures.clone();
                if rule.matches(candidate, document, &mut branch) {
                    matched = Some(branch);
                    break;
                }
            }
            let Some(branch) = matched else {
                return false;
            };
            *captures = branch;
        }
        if let Some(rule) = &self.not {
            let mut branch = captures.clone();
            if rule.matches(candidate, document, &mut branch) {
                return false;
            }
        }
        true
    }
}

struct Document<'a> {
    content: &'a str,
}

fn matches_descendant(
    rule: &CompiledRule,
    candidate: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
    depth: usize,
) -> bool {
    if depth >= MAX_STRUCTURAL_DEPTH {
        return false;
    }
    for index in 0..candidate.named_child_count() {
        if candidate.named_child(index as u32).is_some_and(|child| {
            matches_descendant_candidate(rule, child, document, captures, depth)
        }) {
            return true;
        }
    }
    false
}

fn matches_descendant_candidate(
    rule: &CompiledRule,
    child: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
    depth: usize,
) -> bool {
    if rule.matches_candidate(child) {
        let mut branch = captures.clone();
        if rule.matches(child, document, &mut branch) {
            branch.capture_replace(
                SECONDARY_CAPTURE,
                node_text(child, document.content).to_owned(),
                raw_range(child),
            );
            *captures = branch;
            return true;
        }
    }
    if !rule.stop_by_end {
        return false;
    }
    matches_descendant(rule, child, document, captures, depth + 1)
}

fn matches_ancestor(
    rule: &CompiledRule,
    candidate: Node<'_>,
    document: &Document<'_>,
    captures: &mut CaptureEnv,
) -> bool {
    let mut parent = candidate.parent();
    while let Some(node) = parent {
        if rule.matches_candidate(node) {
            let mut branch = captures.clone();
            if rule.matches(node, document, &mut branch) {
                branch.capture_replace(
                    SECONDARY_CAPTURE,
                    node_text(node, document.content).to_owned(),
                    raw_range(node),
                );
                *captures = branch;
                return true;
            }
        }
        if !rule.stop_by_end {
            return false;
        }
        parent = node.parent();
    }
    false
}

fn first_named_descendant_kind<'tree>(
    node: Node<'tree>,
    kind: &str,
    depth: usize,
) -> Option<Node<'tree>> {
    if depth >= MAX_STRUCTURAL_DEPTH {
        return None;
    }
    for index in 0..node.named_child_count() {
        let child = node.named_child(index as u32)?;
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = first_named_descendant_kind(child, kind, depth + 1) {
            return Some(found);
        }
    }
    None
}

fn effective_pattern_root(mut node: Node<'_>) -> Node<'_> {
    loop {
        let named = named_children(node);
        if named.len() == 1 && is_pattern_wrapper(node.kind()) {
            node = named[0];
            continue;
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
    )
}

fn children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::with_capacity(node.child_count());
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index as u32) {
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

struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(content: &str) -> Self {
        let mut line_starts = vec![0];
        for (index, byte) in content.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self { line_starts }
    }

    fn byte_to_line_col(&self, content: &str, byte: usize) -> (usize, usize) {
        let byte = byte.min(content.len());
        let row = match self.line_starts.binary_search(&byte) {
            Ok(index) => index,
            Err(0) => 0,
            Err(index) => index - 1,
        };
        let line_start = self.line_starts.get(row).copied().unwrap_or_default();
        let byte_column = byte.saturating_sub(line_start);
        (
            row + 1,
            self.point_column_to_char_column(content, row, byte_column),
        )
    }

    /// Convert a tree-sitter byte column to an LSP-compatible **UTF-16 code-unit**
    /// column. This is the unit `lspGetSemantics` uses, the JS resolver emits
    /// (`resolver::byte_offset_to_utf16`), and the signatures layer reports
    /// (`char::len_utf16`). Counting Unicode scalar values (`chars().count()`)
    /// instead would disagree with every other layer on any line containing a
    /// non-BMP character (e.g. an emoji is one code point but two UTF-16 units).
    fn point_column_to_char_column(&self, content: &str, row: usize, byte_column: usize) -> usize {
        let line_start = self.line_starts.get(row).copied().unwrap_or_default();
        let line_end = self
            .line_starts
            .get(row + 1)
            .map(|start| start.saturating_sub(1))
            .unwrap_or(content.len())
            .min(content.len());
        let byte_end = line_start.saturating_add(byte_column).min(line_end);
        content
            .get(line_start..byte_end)
            .map(|slice| slice.chars().map(char::len_utf16).sum::<usize>())
            .unwrap_or(byte_column)
    }
}

/// Converts raw tree-sitter capture positions into `MetavarRange`s (1-based
/// line, char column), pairing each range with its captured text by index.
fn build_metavar_ranges(
    content: &str,
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
                    column: line_index.point_column_to_char_column(
                        content,
                        sr as usize,
                        sc as usize,
                    ) as u32,
                    end_line: er + 1,
                    end_column: line_index.point_column_to_char_column(
                        content,
                        er as usize,
                        ec as usize,
                    ) as u32,
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
    let metavar_ranges = build_metavar_ranges(content, line_index, &metavars, metavar_ranges_raw);
    StructuralMatch {
        start_line: (start.row as u32) + 1,
        end_line: (end.row as u32) + 1,
        start_col: line_index.point_column_to_char_column(content, start.row, start.column) as u32,
        end_col: line_index.point_column_to_char_column(content, end.row, end.column) as u32,
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
    let (start_line, start_col) = line_index.byte_to_line_col(content, start_byte);
    let (end_line, end_col) = line_index.byte_to_line_col(content, end_byte);
    let metavar_ranges = build_metavar_ranges(content, line_index, &metavars, metavar_ranges_raw);
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
#[cfg(test)]
#[path = "octo_tests.rs"]
mod tests;
