use std::collections::HashSet;
use std::time::Instant;

use tree_sitter::{Language, Node, Tree};

use super::error::CompileError;
use super::line_index_support::to_structural_match_with_index;
use super::matching::{
    CandidatePlan, CaptureEnv, ExecutionError, Found, children, named_children, node_text,
    parse_tree, parse_tree_with_deadline, raw_range, visit_named,
};
use super::metavars::{
    MetaVar, ambiguous_function_body_captures, key_value_pair_capture, meta_from_node,
    minimum_candidate_nodes, tag_name_capture,
};
use super::recovery::Recovery;
use crate::structural::kinds::named_kind_id;
use crate::structural::language::{AgLanguage, Expando, WRAP_MARKER};
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

/// See [`CompiledPattern::declaration`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternDeclaration {
    /// The declaration's node kind (`function_item`).
    pub kind: String,
    /// The name node's kind and text, when the pattern spells a name.
    pub name: Option<(String, String)>,
}

pub(super) struct CompiledPattern {
    language: Language,
    expando: Expando,
    source: String,
    tree: Option<Tree>,
    special: Option<SpecialPattern>,
    candidate_plan: CandidatePlan,
    selected_node: Option<(usize, usize, u16)>,
}

/// Pattern text with metavariables substituted for the selected grammar.
#[derive(Clone)]
struct PatternText {
    source: String,
}

impl PatternText {
    fn new(lang: &AgLanguage, pattern: &str) -> Self {
        Self {
            source: lang.preprocess_pattern(pattern).into_owned(),
        }
    }

    fn plain(lang: &AgLanguage, pattern: &str) -> Self {
        Self {
            source: lang.preprocess_metavariables(pattern).into_owned(),
        }
    }

    fn is_terminated(&self) -> bool {
        self.source.trim_end().ends_with([';', '}'])
    }

    fn terminated(&self) -> Self {
        self.map(|text| format!("{text};"))
    }

    fn map(&self, edit: impl Fn(&str) -> String) -> Self {
        Self {
            source: edit(&self.source),
        }
    }

    /// Insert `ch` at byte `offset` of the parser input.
    fn insert(&mut self, offset: usize, ch: char) {
        self.source.insert(offset, ch);
    }
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
    pub(super) fn new(lang: &AgLanguage, pattern: &str) -> Result<Self, CompileError> {
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
                selected_node: None,
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
                selected_node: None,
                special: Some(SpecialPattern::KeyValuePair {
                    key_capture: key_capture.to_owned(),
                    value_capture: value_capture.to_owned(),
                }),
                candidate_plan: CandidatePlan::from_kind("pair"),
            });
        }

        if pattern.len() > 64_000 {
            return Err(CompileError::grammar(
                "structural pattern exceeds 64000 byte limit",
            ));
        }
        let mut text = PatternText::new(lang, pattern);
        let mut tree = parse_tree(&language, &text.source)?;
        // The C# class wrap gives member patterns (methods, properties) real
        // body context, but an expression or statement (`Helper($A)`,
        // `Log($A);`) is not a class member and fails there. Fall back to the
        // plain snippet, whose top-level `global_statement` wrapper
        // `effective_pattern_root` unwraps.
        if lang.class_wraps() {
            let mut plain = PatternText::plain(lang, pattern);
            let mut plain_tree = parse_tree(&language, &plain.source)?;
            if !plain.is_terminated() {
                // A bare call needs a terminator to be a statement.
                let terminated = plain.terminated();
                let terminated_tree = parse_tree(&language, &terminated.source)?;
                if !effective_pattern_root(terminated_tree.root_node(), &terminated.source)
                    .has_error()
                {
                    plain = terminated;
                    plain_tree = terminated_tree;
                }
            }
            let plain_root = effective_pattern_root(plain_tree.root_node(), &plain.source);
            // `local_function_statement` is the top-level artifact of a
            // member-shaped snippet (`public int $NAME(...) { ... }`): keep
            // the class context for it and whenever the plain parse fails.
            if !plain_root.has_error() && plain_root.kind() != "local_function_statement" {
                text = plain;
                tree = plain_tree;
            }
        }
        // Parse fragments once, at compilation, for both direct patterns and
        // every nested YAML pattern. A grammar-checked terminator supplies
        // statement/declaration context without depending on source matches:
        // a call takes it whatever the plain parse made of it (Go reads a
        // bare call as a conversion), and any other fragment (`$A`, `$A?`,
        // `$X.await`, `$A as $T`, `&mut $A`) takes it when the plain snippet
        // does not parse. The statement wrapper unwraps, so `$A;` is `$A`.
        // Complete constructs and unrelated shapes retain their original tree.
        if let Some(kind) = lang.terminated_fragment_kind()
            && !text.is_terminated()
        {
            let contextual = text.terminated();
            let contextual_tree = parse_tree(&language, &contextual.source)?;
            let contextual_root =
                effective_pattern_root(contextual_tree.root_node(), &contextual.source);
            let plain_fails = effective_pattern_root(tree.root_node(), &text.source).has_error();
            if !contextual_root.has_error() && (contextual_root.kind() == kind || plain_fails) {
                text = contextual;
                tree = contextual_tree;
            }
        }
        // A statement only parses cleanly inside a function body (see
        // `AgLanguage::c_family`): reparse a snippet that fails
        // alone as the sole statement of a synthetic function, terminated if
        // it needs it. Declarations, which parse at top level, keep their tree.
        if lang.is_c_family() && effective_pattern_root(tree.root_node(), &text.source).has_error()
        {
            let statements = std::iter::once(text.clone())
                .chain((!text.is_terminated()).then(|| text.terminated()));
            for statement in statements {
                let wrapped = statement.map(|statement| lang.wrap_in_function_body(statement));
                let wrapped_tree = parse_tree(&language, &wrapped.source)?;
                let wrapped_root =
                    effective_pattern_root(wrapped_tree.root_node(), &wrapped.source);
                if !wrapped_root.has_error()
                    && !is_wrap_marker_function(wrapped_root, &wrapped.source)
                {
                    text = wrapped;
                    tree = wrapped_tree;
                    break;
                }
            }
        }
        let ambiguous_bodies = ambiguous_function_body_captures(
            effective_pattern_root(tree.root_node(), &text.source),
            &text.source,
            lang.expando(),
        );
        if !ambiguous_bodies.is_empty() {
            let mut contextual = text.clone();
            for offset in ambiguous_bodies.iter().rev() {
                contextual.insert(*offset, ';');
            }
            let contextual_tree = parse_tree(&language, &contextual.source)?;
            let contextual_root =
                effective_pattern_root(contextual_tree.root_node(), &contextual.source);
            let resolved = ambiguous_function_body_captures(
                contextual_root,
                &contextual.source,
                lang.expando(),
            )
            .is_empty();
            let no_new_errors = !contextual_root.has_error()
                || effective_pattern_root(tree.root_node(), &text.source).has_error();
            if resolved && no_new_errors {
                text = contextual;
                tree = contextual_tree;
            }
        }
        let PatternText { source } = text;
        let root = effective_pattern_root(tree.root_node(), &source);
        if root.is_error() {
            return Err(CompileError::grammar(unparsed_pattern_message(
                root,
                &source,
                lang.expando(),
            )));
        }
        // Several top-level nodes (`a(); b()`) have no single node to match;
        // compiled as a whole file they would silently match only a file
        // starting with that sequence.
        let top_level = named_children(root)
            .into_iter()
            .filter(|node| !node.is_extra())
            .count();
        if is_pattern_wrapper(root.kind()) && top_level > 1 {
            return Err(CompileError::grammar(format!(
                "invalid structural pattern: it has {top_level} top-level nodes, but a pattern matches one node. Search for one statement, or use a rule with `has`/`inside` to relate them."
            )));
        }
        let is_meta = meta_from_node(root, &source, lang.expando()).is_some();
        let candidate_plan = if is_meta {
            CandidatePlan::Any
        } else {
            CandidatePlan::from_kind(root.kind())
        };
        Ok(Self {
            language,
            expando: lang.expando(),
            source,
            tree: Some(tree),
            selected_node: None,
            special: None,
            candidate_plan,
        })
    }

    pub(super) fn contextual(
        lang: &AgLanguage,
        context: &str,
        selector: &str,
    ) -> Result<Self, CompileError> {
        if context.len() > 64_000 {
            return Err(CompileError::rule(
                "structural pattern exceeds 64000 byte limit",
            ));
        }
        let language = lang.tree_sitter_language();
        let kind = named_kind_id(&language, selector).ok_or_else(|| {
            CompileError::grammar(format!(
                "unknown selector kind '{selector}' for this language grammar"
            ))
        })?;
        let source = lang.preprocess_metavariables(context).into_owned();
        let tree = parse_tree(&language, &source)?;
        if tree.root_node().has_error() {
            return Err(CompileError::grammar(
                "contextual pattern must parse without syntax errors",
            ));
        }
        let mut pending = vec![tree.root_node()];
        let mut selected = None;
        while let Some(node) = pending.pop() {
            if node.kind_id() == kind {
                selected = Some((node.start_byte(), node.end_byte(), node.kind_id()));
                break;
            }
            pending.extend(children(node).into_iter().rev());
        }
        let selected_node = selected.ok_or_else(|| {
            CompileError::grammar(format!("context contains no '{selector}' node"))
        })?;
        Ok(Self {
            language,
            expando: lang.expando(),
            source,
            tree: Some(tree),
            special: None,
            candidate_plan: CandidatePlan::from_kind(selector),
            selected_node: Some(selected_node),
        })
    }

    fn root(&self) -> Option<Node<'_>> {
        let tree = self.tree.as_ref()?;
        let Some((start, end, kind)) = self.selected_node else {
            return Some(effective_pattern_root(tree.root_node(), &self.source));
        };
        let mut node = tree
            .root_node()
            .named_descendant_for_byte_range(start, end)?;
        while node.kind_id() != kind || node.start_byte() != start || node.end_byte() != end {
            node = node.parent()?;
        }
        Some(node)
    }

    pub(super) fn language(&self) -> &Language {
        &self.language
    }

    /// The declaration a pattern spells: its root's kind when the grammar
    /// gives that node a `name` field, with the name node's kind and text
    /// unless the name is a metavariable. `None` for any other pattern.
    pub(super) fn declaration(&self) -> Option<PatternDeclaration> {
        let root = self.root()?;
        let name = root.child_by_field_name("name")?;
        let named = meta_from_node(name, &self.source, self.expando)
            .is_none()
            .then(|| {
                (
                    name.kind().to_owned(),
                    node_text(name, &self.source).to_owned(),
                )
            });
        Some(PatternDeclaration {
            kind: root.kind().to_owned(),
            name: named,
        })
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
        recovery: &Recovery,
        found: &mut Found<'_>,
    ) -> Result<(), ExecutionError> {
        let Some(special) = &self.special else {
            return Ok(());
        };
        let tree = parse_tree_with_deadline(&self.language, content, deadline)?;
        if recovery.hides_match(&tree, content) {
            found.mark_recovered();
        }

        // One match per node span.
        let mut seen = HashSet::new();
        let line_index = LineIndex::tree_sitter(content);
        visit_named(tree.root_node(), deadline, &mut |candidate| {
            if !self.matches_candidate(candidate) {
                return Ok(());
            }
            // Direct patterns and pattern rules share capture equality semantics.
            let mut captures = CaptureEnv::default();
            if self.matches_special(special, candidate, content, &mut captures)
                && seen.insert((candidate.start_byte(), candidate.end_byte()))
            {
                found.push(candidate, || {
                    let (metavars, metavar_ranges_raw) = captures.into_maps();
                    to_structural_match_with_index(
                        candidate,
                        content,
                        &line_index,
                        metavars,
                        metavar_ranges_raw,
                    )
                });
            }
            Ok(())
        })
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

        let Some(root) = self.root() else {
            return Ok(false);
        };
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
            // ast-grep "smart" strictness, which rewrite uses: once every
            // pattern node matched, trailing candidate nodes (a trailing
            // argument comma, a `;` the pattern omitted) do not fail it.
            let Some(first) = pattern_children.first().copied() else {
                break true;
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
                // Smart strictness again: every pattern node must match, but
                // candidate trivia the pattern did not spell (punctuation,
                // comments) is skipped rather than failing the match.
                if is_skippable_candidate(candidate_first) {
                    candidate_children = &candidate_children[1..];
                    continue;
                }
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
        if self.match_multi_split(
            name,
            remaining_pattern,
            pattern_source,
            candidate_children,
            candidate_source,
            captures,
            depth,
            budget,
            0,
        )? {
            return Ok(true);
        }
        // As in ast-grep, punctuation the pattern spells right after `$$$` is
        // optional: `f($$$A, x)` matches `f(x)` and `f(1, $$$M, 3)` matches
        // `f(1, 3)`. The skipped tokens never extend the capture.
        let skipped = remaining_pattern
            .iter()
            .take_while(|node| !node.is_named())
            .count();
        if skipped == 0 {
            return Ok(false);
        }
        self.match_multi_split(
            name,
            &remaining_pattern[skipped..],
            pattern_source,
            candidate_children,
            candidate_source,
            captures,
            depth,
            budget,
            skipped,
        )
    }

    /// Try every split of `candidate_children` between a `$$$` capture and
    /// `remaining_pattern`, shortest capture first. A trailing `$$$` takes
    /// every remaining candidate. Up to `trim` anonymous tokens at the end
    /// of a taken run stay out of the capture.
    #[expect(
        clippy::too_many_arguments,
        reason = "the recursive matcher threads its state flat on purpose (see clippy.toml)"
    )]
    fn match_multi_split(
        &self,
        name: Option<&str>,
        remaining_pattern: &[Node<'_>],
        pattern_source: &str,
        candidate_children: &[Node<'_>],
        candidate_source: &str,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
        trim: usize,
    ) -> Result<bool, ExecutionError> {
        let min_remaining =
            minimum_candidate_nodes(remaining_pattern, pattern_source, self.expando);
        if candidate_children.len() < min_remaining {
            return Ok(false);
        }
        let max_take = candidate_children.len() - min_remaining;
        let min_take = if remaining_pattern.is_empty() {
            max_take
        } else {
            0
        };
        for take in min_take..=max_take {
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
                let mut taken = &candidate_children[..take];
                for _ in 0..trim {
                    match taken.split_last() {
                        Some((last, rest)) if !last.is_named() => taken = rest,
                        _ => break,
                    }
                }
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

/// Candidate nodes a pattern need not spell: anonymous tokens and extras
/// such as comments (ast-grep's smart strictness).
fn is_skippable_candidate(node: Node<'_>) -> bool {
    !node.is_named() || node.is_extra()
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

/// Whether `node` is the synthetic C-family function a statement pattern is
/// parsed inside (see `AgLanguage::c_family`), named [`WRAP_MARKER`].
fn is_wrap_marker_function(node: Node<'_>, source: &str) -> bool {
    node.kind() == "function_definition"
        && node
            .child_by_field_name("declarator")
            .and_then(|declarator| declarator.child_by_field_name("declarator"))
            .is_some_and(|name| node_text(name, source) == WRAP_MARKER)
}

/// The node a pattern matches: its parse with every synthetic or
/// single-child wrapper removed. Wrappers we add are recognised by
/// [`WRAP_MARKER`] (rather than by kind, like the grammar's own wrappers), so
/// a real `class $NAME { ... }` or function pattern keeps its outer node.
pub(super) fn effective_pattern_root<'a>(mut node: Node<'a>, source: &str) -> Node<'a> {
    loop {
        let named = named_children(node);
        if named.len() == 1 && is_pattern_wrapper(node.kind()) {
            node = named[0];
            continue;
        }
        // C-family synthetic function: unwrap `void __OctoWrap(void) {
        // <statement> }` down to its single statement.
        if is_wrap_marker_function(node, source)
            && let Some(body) = node.child_by_field_name("body")
        {
            let body_named = named_children(body);
            if body_named.len() == 1 {
                node = body_named[0];
                continue;
            }
        }
        // C#'s synthetic wrapper class: unwrap `class __OctoWrap { <member> }`
        // down to the single real member, giving it real class-body context
        // (a bare `public int Foo(...) { ... }` parsed standalone isn't valid
        // C# at all — no top-level member/method syntax exists outside a
        // type body).
        if node.kind() == "class_declaration" {
            let is_wrapper = node
                .child_by_field_name("name")
                .is_some_and(|n| node_text(n, source) == WRAP_MARKER);
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
            // C# wraps every top-level statement of a snippet in one.
            | "global_statement"
            | "config_file"
            | "body"
    )
}

/// The rejection for a pattern whose root did not parse. It names the last
/// token the parser could not place (`$$$BODY` in `fn $N() $$$BODY`, where a
/// function needs its body), so the caller sees what to complete.
fn unparsed_pattern_message(root: Node<'_>, source: &str, expando: Expando) -> String {
    const BASE: &str = "invalid structural pattern: pattern parsed with syntax errors";
    let near = children(root)
        .into_iter()
        .rev()
        .find(|node| !node.is_extra())
        .map(|node| expando.restore(node_text(node, source).trim()))
        .filter(|text| !text.is_empty());
    match near {
        Some(text) => format!("{BASE} near `{text}`"),
        None => BASE.to_owned(),
    }
}
