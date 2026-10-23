//! Matching a compiled pattern node against a candidate node.

use tree_sitter::Node;

use crate::structural::octo::matching::{
    CaptureEnv, ExecutionError, children, node_text, raw_range,
};
use crate::structural::octo::metavars::{MetaVar, meta_from_node, minimum_candidate_nodes};

use super::{CompiledPattern, MatchBudget};

/// Native recursion guard for pattern matching; AST traversal is iterative.
const MAX_STRUCTURAL_DEPTH: usize = 500;

impl CompiledPattern {
    // `depth` guards native-stack growth against pathologically nested patterns
    // (see `MAX_STRUCTURAL_DEPTH`); `attempts` is the shared `$$$` split budget
    // (see `MAX_MULTI_CAPTURE_ATTEMPTS`). The budget also carries the run deadline.
    pub(super) fn match_node(
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
