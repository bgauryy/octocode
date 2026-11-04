//! Matching a compiled pattern node against a candidate node.

use tree_sitter::Node;

use crate::structural::language::Expando;
use crate::structural::octo::matching::{
    CaptureEnv, ExecutionError, children, node_text, raw_range,
};
use crate::structural::octo::metavars::{MetaVar, meta_from_node};

use super::unsaid::{Declaration, ParameterSlot, Unsaid, is_longer_form, wrapped_capture};
use super::{CompiledPattern, MatchBudget};

mod multi;

/// Native recursion guard for pattern matching; AST traversal is iterative.
const MAX_STRUCTURAL_DEPTH: usize = 500;

/// The source text of each side of a match.
#[derive(Clone, Copy)]
pub(super) struct Texts<'s> {
    pub(super) pattern: &'s str,
    pub(super) candidate: &'s str,
}

impl CompiledPattern {
    // `depth` guards native-stack growth against pathologically nested patterns
    // (see `MAX_STRUCTURAL_DEPTH`); `attempts` is the shared `$$$` split budget
    // (see `MAX_MULTI_CAPTURE_ATTEMPTS`). The budget also carries the run deadline.
    pub(super) fn match_node(
        &self,
        pattern: Node<'_>,
        candidate: Node<'_>,
        texts: Texts<'_>,
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
        if let Some(meta) = meta_from_node(pattern, texts.pattern, self.expando) {
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
                    node_text(candidate, texts.candidate),
                    raw_range(candidate),
                ),
                MetaVar::IgnoredSingle => true,
                MetaVar::Multi(_) | MetaVar::IgnoredMulti => false,
            });
        }

        if pattern.kind() != candidate.kind() && !is_longer_form(pattern, candidate.kind()) {
            return Ok(false);
        }
        let (pattern_children, candidate_children) =
            child_lists(pattern, candidate, texts.pattern, self.expando);
        if pattern_children.is_empty() && candidate_children.is_empty() {
            return Ok(node_text(pattern, texts.pattern) == node_text(candidate, texts.candidate));
        }
        let unsaid = Unsaid::of(pattern, candidate, texts.pattern, self.expando);
        self.match_child_list(
            &pattern_children,
            &candidate_children,
            &unsaid,
            texts,
            captures,
            depth,
            budget,
        )
    }

    /// One pattern child against one candidate child of a declaration: a type's
    /// body reads its members as held, a declarator the function under it, a
    /// capture's wrapper the other form of its slot, anything else as itself.
    fn match_declaration_part(
        &self,
        declaration: &Declaration,
        part: Node<'_>,
        candidate: Node<'_>,
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        if declaration.is_member_body(part) {
            if part.kind() != candidate.kind() {
                return Ok(false);
            }
            let (pattern_children, candidate_children) =
                child_lists(part, candidate, texts.pattern, self.expando);
            return self.match_child_list(
                &pattern_children,
                &candidate_children,
                &Unsaid::Members,
                texts,
                captures,
                depth + 1,
                budget,
            );
        }
        let candidate = declaration
            .under_declarator(part, candidate)
            .unwrap_or(candidate);
        if let Some((capture, wrapped)) = wrapped_capture(
            part,
            candidate,
            (texts.pattern, texts.candidate),
            self.expando,
        ) {
            return self.match_node(capture, wrapped, texts, captures, depth + 1, budget);
        }
        self.match_node(part, candidate, texts, captures, depth + 1, budget)
    }

    fn match_child_list(
        &self,
        mut pattern_children: &[Node<'_>],
        mut candidate_children: &[Node<'_>],
        unsaid: &Unsaid,
        texts: Texts<'_>,
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
            let multi = match meta_from_node(first, texts.pattern, self.expando) {
                Some(MetaVar::Multi(name)) => Some(name),
                Some(MetaVar::IgnoredMulti) => Some(None),
                _ => None,
            };
            if let Some(name) = multi {
                break self.match_multi_capture(
                    name,
                    &pattern_children[1..],
                    candidate_children,
                    unsaid,
                    texts,
                    captures,
                    depth + 1,
                    budget,
                )?;
            }
            let candidate_first = candidate_children.first().copied();
            if let Unsaid::Declaration(declaration) = unsaid {
                match declaration.parameter_slot(
                    first,
                    candidate_first,
                    texts.pattern,
                    self.expando,
                ) {
                    ParameterSlot::Usual => {}
                    ParameterSlot::Absent(name) => {
                        if !bind_parameters(name, &[], texts, captures) {
                            break false;
                        }
                        pattern_children = &pattern_children[1..];
                        continue;
                    }
                    ParameterSlot::Spelled(name) => {
                        if !bind_parameters(name, &candidate_children[..1], texts, captures) {
                            break false;
                        }
                        pattern_children = &pattern_children[1..];
                        candidate_children = &candidate_children[1..];
                        continue;
                    }
                }
            }
            let Some(candidate_first) = candidate_first else {
                break false;
            };
            let (part_matched, skips) = match unsaid {
                Unsaid::Exact => (
                    self.match_node(first, candidate_first, texts, captures, depth + 1, budget)?,
                    is_skippable_candidate(candidate_first),
                ),
                Unsaid::Members => (
                    self.match_node(first, candidate_first, texts, captures, depth + 1, budget)?,
                    true,
                ),
                Unsaid::Declaration(declaration) => {
                    let skips = is_skippable_candidate(candidate_first)
                        || declaration.leaves_unsaid(candidate_first, pattern_children);
                    // A part in a field reads only the candidate's child in
                    // that field: a capture does not bind the modifier or
                    // token before it.
                    let other_slot = declaration.field_of_pattern(first).is_some_and(|field| {
                        declaration.field_of_candidate(candidate_first) != Some(field)
                    });
                    let matched = !other_slot
                        && self.match_declaration_part(
                            declaration,
                            first,
                            candidate_first,
                            texts,
                            captures,
                            depth,
                            budget,
                        )?;
                    (matched, skips)
                }
            };
            if !part_matched {
                // Smart strictness again: every pattern node must match, but
                // what the pattern did not spell (punctuation, comments, a
                // declaration's unsaid parts) is skipped rather than failing
                // the match.
                if skips {
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
}

/// Each node's children to compare.
///
/// MISSING nodes drop from the PATTERN's own children. A MISSING node is
/// tree-sitter's zero-width error-recovery stand-in for a token the grammar
/// expected but never got, which never represents user intent (nobody types a
/// "missing token" into a pattern string). It shows up because some grammars
/// parse a bare `$$$NAME`/`$X` expando identifier at statement position
/// ambiguously (C/C++/C# treat an unrecognized identifier as the start of a
/// declaration and then expect a trailing `;`): the compiled pattern's root is
/// not `is_error()`, but the MISSING sibling it left behind could never match
/// a real candidate child. The candidate side is left as-is: a MISSING token
/// in the searched document is genuine evidence of broken source and must
/// still fail to match.
///
/// So does a lone `;` right after a `$$$`: the terminator a grammar needed to
/// parse the capture (`CompiledPattern::new`), which the caller did not write.
fn child_lists<'p, 'c>(
    pattern: Node<'p>,
    candidate: Node<'c>,
    source: &str,
    expando: Expando,
) -> (Vec<Node<'p>>, Vec<Node<'c>>) {
    let mut after_multi = false;
    let pattern_children = children(pattern)
        .into_iter()
        .filter(|node| {
            if node.is_missing() {
                return false;
            }
            let terminator = after_multi && node_text(*node, source) == ";";
            after_multi = matches!(
                meta_from_node(*node, source, expando),
                Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
            );
            !terminator
        })
        .collect();
    (pattern_children, children(candidate))
}

/// Fill an any-parameters list with the nodes the candidate holds there.
fn bind_parameters(
    name: Option<&str>,
    nodes: &[Node<'_>],
    texts: Texts<'_>,
    captures: &mut CaptureEnv,
) -> bool {
    name.is_none_or(|name| {
        captures.capture_many(
            name,
            nodes.iter().map(|node| node_text(*node, texts.candidate)),
            || nodes.iter().map(|node| raw_range(*node)).collect(),
        )
    })
}

/// Candidate nodes a pattern need not spell: anonymous tokens and extras
/// such as comments (ast-grep's smart strictness).
fn is_skippable_candidate(node: Node<'_>) -> bool {
    !node.is_named() || node.is_extra()
}
