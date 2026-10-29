//! A bounded final collection tail can use the space its finished envelope frees.
use super::{DependencyError, FragmentArena, Open, Planned, Slicer, continues_chars};

impl Slicer<'_> {
    pub(super) fn prepare_terminal(
        &mut self,
        arena: &FragmentArena,
        index: usize,
        remaining: usize,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        let Some(budget) = self.window.terminal_budget else {
            return Ok(());
        };
        if self.terminal_checked || out.is_empty() {
            return Ok(());
        }
        let marker = self
            .enclosing
            .as_ref()
            .map_or(0, |(array, index)| continues_chars(array, *index));
        let limit = budget
            .saturating_sub(self.window.continuation_chars.max(marker) + self.window.tail)
            .max(1);
        // Never enlarge the first fragment: it retains the first-only fields
        // and fair-share head. Begin a new chunk after that fragment instead.
        let fresh = matches!(self.open, Open::Tail { at: 0, .. });
        let context = &arena.splits[self.split].contexts[usize::from(index > 0)];
        let (chars, evidence) = if fresh {
            (context.chars, &context.evidence)
        } else {
            let chars = match self.open {
                Open::Chunk { chars, .. } | Open::Tail { chars, .. } => chars,
            };
            (chars, &self.open_evidence)
        };
        if chars.saturating_add(remaining) > limit {
            return Ok(());
        }
        // At most one descriptor-union trial per collection. A failed trial
        // is conservative; it neither changes the open fragment nor retries
        // increasingly short suffixes. Raw suffix size is maintained by push.
        self.terminal_checked = true;
        let mut selected = evidence.clone();
        for added in &arena.splits[self.split].evidence[index..] {
            self.window.check()?;
            arena.merge_evidence(&mut selected, added, self.window.cancelled)?;
            let extra = arena.evidence_delta(evidence, &selected, self.window.cancelled)?;
            if chars.saturating_add(remaining).saturating_add(extra) > limit {
                return Ok(());
            }
        }
        if fresh {
            self.close(arena, index, out);
        }
        self.terminal_limit = Some(limit);
        Ok(())
    }
}
