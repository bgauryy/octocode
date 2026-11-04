//! Matching a `$$$` capture: which run of a candidate's children it takes.

use tree_sitter::Node;

use crate::structural::octo::matching::{CaptureEnv, ExecutionError, node_text, raw_range};
use crate::structural::octo::metavars::minimum_candidate_nodes;

use super::super::unsaid::Unsaid;
use super::super::{CompiledPattern, MatchBudget};
use super::Texts;

impl CompiledPattern {
    pub(super) fn match_multi_capture(
        &self,
        name: Option<&str>,
        remaining_pattern: &[Node<'_>],
        candidate_children: &[Node<'_>],
        unsaid: &Unsaid,
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
    ) -> Result<bool, ExecutionError> {
        if self.match_multi_split(
            name,
            remaining_pattern,
            candidate_children,
            unsaid,
            texts,
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
            candidate_children,
            unsaid,
            texts,
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
        candidate_children: &[Node<'_>],
        unsaid: &Unsaid,
        texts: Texts<'_>,
        captures: &mut CaptureEnv,
        depth: usize,
        budget: &mut MatchBudget,
        trim: usize,
    ) -> Result<bool, ExecutionError> {
        let min_remaining = minimum_candidate_nodes(remaining_pattern, texts.pattern, self.expando);
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
                let values = taken.iter().map(|node| node_text(*node, texts.candidate));
                if !captures.capture_many(name, values, || {
                    taken.iter().map(|node| raw_range(*node)).collect()
                }) {
                    continue;
                }
            }
            if self.match_child_list(
                remaining_pattern,
                &candidate_children[take..],
                unsaid,
                texts,
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
