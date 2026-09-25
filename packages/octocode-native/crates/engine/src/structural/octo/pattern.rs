use std::collections::HashSet;
use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use crate::structural::kinds::named_kind_id;
use crate::structural::language::{AgLanguage, Expando};
use crate::structural::metavars::{
    MetaVar, ambiguous_function_body_capture, key_value_pair_capture, meta_from_node,
    minimum_candidate_nodes, tag_name_capture,
};
use crate::structural::types::StructuralMatch;

use super::line_index_support::to_structural_match_with_index;
use super::matching::{
    CandidatePlan, CaptureEnv, ExecutionError, MatchWithKind, children, named_children, node_text,
    parse_tree, parse_tree_with_deadline, raw_range, visit_named,
};
use crate::text::utf8_offsets::LineIndex;

/// Native recursion guard for pattern matching; AST traversal is iterative.
const MAX_STRUCTURAL_DEPTH: usize = 500;

/// Budget on `$$$` multi-metavar split attempts within a single top-level
/// child-list match. Each `$$$` tries every split point (`for take in
/// 0..=max_take`) and several `$$$` against a wide node are combinatorial; this
/// caps the total work so a crafted pattern + wide input can't stall for
/// seconds. Exhaustion is an explicit incomplete execution error.
const MAX_MULTI_CAPTURE_ATTEMPTS: usize = 10_000;

pub(super) struct MatchBudget {
    attempts: usize,
    deadline: Instant,
}

pub(super) struct CompiledPattern {
    language: Language,
    expando: Expando,
    source: String,
    tree: Option<Tree>,
    special: Option<SpecialPattern>,
    candidate_plan: CandidatePlan,
}

enum SpecialPattern {
    /// `<$T>` over a JSX-capable grammar: opening and self-closing elements.
    JsxTagName { capture: String },
    KeyValuePair {
        key_capture: String,
        value_capture: String,
    },
}

impl CompiledPattern {
    pub(super) fn new(lang: &AgLanguage, pattern: &str) -> Result<Self, String> {
        // Special shapes are gated on the grammar actually having the target
        // node kinds; elsewhere the text compiles as an ordinary pattern.
        let language = lang.tree_sitter_language();
        if let Some(capture) = tag_name_capture(pattern)
            && named_kind_id(&language, "jsx_opening_element").is_some()
        {
            return Ok(Self {
                language,
                expando: lang.expando(),
                source: pattern.to_owned(),
                tree: None,
                special: Some(SpecialPattern::JsxTagName {
                    capture: capture.to_owned(),
                }),
                candidate_plan: CandidatePlan::from_kinds(JSX_TAG_KINDS.map(str::to_owned)),
            });
        }

        if let Some((key_capture, value_capture)) = key_value_pair_capture(pattern)
            && named_kind_id(&language, "pair").is_some()
        {
            return Ok(Self {
                language,
                expando: lang.expando(),
                source: pattern.to_owned(),
                tree: None,
                special: Some(SpecialPattern::KeyValuePair {
                    key_capture: key_capture.to_owned(),
                    value_capture: value_capture.to_owned(),
                }),
                candidate_plan: CandidatePlan::from_kind("pair"),
            });
        }

        if pattern.len() > 64_000 {
            return Err("structural pattern exceeds 64000 byte limit".to_owned());
        }
        let mut source = lang.preprocess_pattern(pattern).into_owned();
        let mut tree = parse_tree(&language, &source).map_err(|err| err.to_string())?;
        // Parse fragments once, at compilation, for both direct patterns and
        // every nested YAML pattern. A grammar-checked terminator supplies
        // statement/declaration context without depending on source matches.
        // Complete constructs and unrelated shapes retain their original tree.
        if let Some(kind) = lang.terminated_fragment_kind()
            && !source.trim_end().ends_with([';', '}'])
        {
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

    pub(super) fn language(&self) -> &Language {
        &self.language
    }

    pub(super) fn is_special(&self) -> bool {
        self.special.is_some()
    }

    pub(super) fn candidate_plan(&self) -> &CandidatePlan {
        &self.candidate_plan
    }

    pub(super) fn matches_candidate(&self, candidate: Node<'_>) -> bool {
        self.candidate_plan.matches(candidate)
    }

    pub(super) fn find_special_matches(
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

    pub(super) fn matches(
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
            SpecialPattern::JsxTagName { .. } | SpecialPattern::KeyValuePair { .. } => {
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
            SpecialPattern::JsxTagName { capture } => {
                let Some(tag_name) = jsx_tag_name_node(candidate) else {
                    return false;
                };
                captures.capture_one(capture, node_text(tag_name, content), raw_range(tag_name))
            }
            SpecialPattern::KeyValuePair {
                key_capture,
                value_capture,
            } => {
                let Some((key, value)) = key_value_nodes(candidate) else {
                    return false;
                };
                let checkpoint = captures.checkpoint();
                let matched =
                    captures.capture_one(key_capture, node_text(key, content), raw_range(key))
                        && captures.capture_one(
                            value_capture,
                            node_text(value, content),
                            raw_range(value),
                        );
                if !matched {
                    captures.rollback(checkpoint);
                }
                matched
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
                    name,
                    node_text(candidate, candidate_source),
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
        // Bind straight into `captures`; any failure rolls back to here so a
        // `false` result leaves the caller's environment unchanged.
        let checkpoint = captures.checkpoint();
        let matched = loop {
            let Some(first) = pattern_children.first().copied() else {
                break candidate_children.is_empty();
            };
            let multi = match meta_from_node(first, pattern_source, self.expando) {
                Some(MetaVar::Multi(name)) => Some(name),
                Some(MetaVar::IgnoredMulti) => Some(None),
                _ => None,
            };
            if let Some(name) = multi {
                break self.match_multi_capture(
                    name,
                    &pattern_children[1..],
                    pattern_source,
                    candidate_children,
                    candidate_source,
                    captures,
                    depth + 1,
                    budget,
                )?;
            }
            let Some(candidate_first) = candidate_children.first().copied() else {
                break false;
            };
            if !self.match_node(
                first,
                pattern_source,
                candidate_first,
                candidate_source,
                captures,
                depth + 1,
                budget,
            )? {
                break false;
            }
            pattern_children = &pattern_children[1..];
            candidate_children = &candidate_children[1..];
        };
        if !matched {
            captures.rollback(checkpoint);
        }
        Ok(matched)
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
            let checkpoint = captures.checkpoint();
            if let Some(name) = name {
                let taken = &candidate_children[..take];
                let texts = taken.iter().map(|node| node_text(*node, candidate_source));
                if !captures.capture_many(name, texts, || {
                    taken.iter().map(|node| raw_range(*node)).collect()
                }) {
                    continue;
                }
            }
            if self.match_child_list(
                remaining_pattern,
                pattern_source,
                &candidate_children[take..],
                candidate_source,
                captures,
                depth,
                budget,
            )? {
                return Ok(true);
            }
            captures.rollback(checkpoint);
        }
        Ok(false)
    }
}

/// Opening/self-closing JSX elements: the nodes `<$T>` matches.
const JSX_TAG_KINDS: [&str; 2] = ["jsx_opening_element", "jsx_self_closing_element"];

fn jsx_tag_name_node(candidate: Node<'_>) -> Option<Node<'_>> {
    if !JSX_TAG_KINDS.contains(&candidate.kind()) {
        return None;
    }
    candidate.child_by_field_name("name")
}

fn key_value_nodes(candidate: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    if candidate.kind() != "pair" {
        return None;
    }
    let named = named_children(candidate);
    match named.as_slice() {
        [key, value, ..] => Some((*key, *value)),
        _ => None,
    }
}

/// Synthetic class name `preprocess_pattern` wraps every C# pattern in — see
/// `AgLanguage::class_wrap`. Real user patterns essentially never target a
/// class literally named this, so matching on it by name (rather than by
/// kind, like the other wrapper cases below) is safe: it only unwraps
/// *our own* synthetic wrapper, never a real `class $NAME { ... }` pattern
/// whose outer class_declaration the user actually wants to match.
pub(super) const CSHARP_WRAP_MARKER: &str = "__OctoWrap";

pub(super) fn effective_pattern_root<'a>(mut node: Node<'a>, source: &str) -> Node<'a> {
    loop {
        let named = named_children(node);
        if named.len() == 1 && is_pattern_wrapper(node.kind()) {
            node = named[0];
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
            if is_wrapper && let Some(body) = node.child_by_field_name("body") {
                let body_named = named_children(body);
                if body_named.len() == 1 {
                    node = body_named[0];
                    continue;
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
