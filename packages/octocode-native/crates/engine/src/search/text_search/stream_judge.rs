//! The rules of a stream's walk ([`OrderedWalk`]): which entries the walk
//! leaves out, silently or reported to the path filter, which files it hands
//! to the search, and when it must stop. They are the rules the `ignore`
//! walker applied together with the search's own: the walker's globs and
//! `language` type drop an entry first ([`Selection`]), then the ignore and
//! dot rules ([`EntryRule`]), then the path filter.

use super::TextSearchPathFilter;
use super::state::CollectState;
use super::visit::{WalkScope, report_left_out};
use super::walk_rule::Skipped;
use crate::search::walk::{DirRules, Judge, ListedRules, Probe, Step};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// What the rules say of an entry before the walk reaches it (no effect).
#[derive(Clone, Copy)]
pub(super) enum Verdict {
    /// Dropped inside the walker: not reported.
    Pruned,
    /// Left out by an ignore or dot rule: reported to the path filter.
    Left(Skipped),
    Open,
}

pub(super) struct StreamJudge<'a> {
    scope: &'a WalkScope<'a>,
    filter: &'a Arc<dyn TextSearchPathFilter>,
}

impl<'a> StreamJudge<'a> {
    pub(super) fn new(scope: &'a WalkScope<'a>, filter: &'a Arc<dyn TextSearchPathFilter>) -> Self {
        Self { scope, filter }
    }

    fn state(&self) -> &CollectState {
        self.scope.state
    }
}

impl Judge for StreamJudge<'_> {
    type Verdict = Verdict;

    fn rules(&self, dir: &Path, listed: Option<ListedRules>) -> Option<DirRules> {
        self.scope.rule.as_ref()?.enter(dir, listed)
    }

    fn verdict(&self, rules: Option<&DirRules>, entry: Probe<'_>) -> Verdict {
        // The root is never left out by a rule.
        if entry.depth == 0 {
            return Verdict::Open;
        }
        let is_dir = entry.is_dir();
        if self.scope.selection.prunes(entry.path, is_dir) {
            return Verdict::Pruned;
        }
        match self
            .scope
            .rule
            .as_deref()
            .and_then(|rule| rule.skips(rules, entry, is_dir))
        {
            Some(skipped) => Verdict::Left(skipped),
            None => Verdict::Open,
        }
    }

    fn opens(&self, verdict: Verdict) -> bool {
        matches!(verdict, Verdict::Open)
    }

    fn judge(&mut self, entry: Probe<'_>, verdict: Verdict) -> Step {
        let is_dir = entry.is_dir();
        match verdict {
            Verdict::Pruned => return Step::Skip,
            Verdict::Left(skipped) => {
                report_left_out(&**self.filter, skipped, entry.path, is_dir);
                return Step::Skip;
            }
            Verdict::Open => {}
        }
        if !self.filter.allows_entry(entry.path, entry.file_type) {
            // A root the filter leaves out leaves nothing to walk.
            return if entry.depth == 0 && is_dir {
                Step::End
            } else {
                Step::Skip
            };
        }
        if is_dir {
            self.scope.note_depth_cut(entry, true);
            return Step::Enter;
        }
        if entry.file_type.is_some_and(|kind| kind.is_file()) {
            return Step::Emit;
        }
        if entry.file_type.is_some_and(|kind| kind.is_symlink()) {
            self.filter.skipped_symlink(entry.path);
        }
        Step::Skip
    }

    fn failed(&mut self, error: String) {
        self.state().record_error(error);
    }

    fn halted(&mut self) -> bool {
        self.scope.halted() || self.state().cancelled.load(Ordering::Relaxed)
    }
}
