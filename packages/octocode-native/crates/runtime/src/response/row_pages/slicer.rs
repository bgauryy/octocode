use super::arena::{Evidence, FragmentKind, Window};
use super::evidence::DependencyError;
use super::{Fragment, FragmentArena, Planned, continues_chars, mark_continuations};
use serde_json::Value;
use std::ops::Range;

mod terminal;

#[derive(Clone, Copy)]
pub(super) enum Open {
    Chunk {
        start: usize,
        chars: usize,
    },
    Tail {
        at: usize,
        start: usize,
        chars: usize,
    },
}

pub(super) struct Slicer<'a> {
    pub(super) split: usize,
    pub(super) enclosing: Option<(String, usize)>,
    pub(super) pointer: String,
    pub(super) first: usize,
    pub(super) window: Window<'a>,
    pub(super) catalog: Option<usize>,
    pub(super) open: Open,
    pub(super) open_evidence: Evidence,
    pub(super) remaining_chars: usize,
    pub(super) terminal_limit: Option<usize>,
    pub(super) terminal_checked: bool,
}

impl Slicer<'_> {
    /// The room of fragment `at` when it holds item `index`, less the tail
    /// room when that item ends the split.
    fn limit(&self, arena: &FragmentArena, at: usize, index: usize) -> usize {
        if let Some(limit) = self.terminal_limit {
            return limit;
        }
        let room = if at == self.first {
            self.window.head
        } else {
            self.window.later_limit(
                self.enclosing
                    .as_ref()
                    .map_or(0, |(array, index)| continues_chars(array, *index)),
            )
        };
        let len = arena.splits[self.split].items.len();
        room.saturating_sub(self.window.tail_of(index, len)).max(1)
    }

    fn start_chunk(
        &mut self,
        arena: &FragmentArena,
        index: usize,
        item: usize,
        added: &Evidence,
    ) -> Result<(), DependencyError> {
        let context = &arena.splits[self.split].contexts[usize::from(index > 0)];
        self.open_evidence = context.evidence.clone();
        let chars = context.chars
            + item
            + arena.evidence_delta(&self.open_evidence, added, self.window.cancelled)?;
        arena.merge_evidence(&mut self.open_evidence, added, self.window.cancelled)?;
        self.open = Open::Chunk {
            start: index,
            chars,
        };
        Ok(())
    }

    pub(super) fn push(
        &mut self,
        arena: &mut FragmentArena,
        index: usize,
        item: usize,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        self.window.check()?;
        let remaining = self.remaining_chars;
        self.remaining_chars = self.remaining_chars.saturating_sub(item);
        self.prepare_terminal(arena, index, remaining, out)?;
        let added = arena.splits[self.split].evidence[index].clone();
        let base = &arena.splits[self.split].contexts[usize::from(index > 0)];
        if base.chars
            + item
            + arena.evidence_delta(&base.evidence, &added, self.window.cancelled)?
            > self.limit(arena, out.len(), index)
        {
            return self.oversized(arena, index, out);
        }
        let delta =
            item + arena.evidence_delta(&self.open_evidence, &added, self.window.cancelled)?;
        match self.open {
            Open::Chunk { start, chars }
                if index > start && chars + delta > self.limit(arena, out.len(), index) =>
            {
                self.flush(start, index, chars, out);
                self.start_chunk(arena, index, item, &added)?;
            }
            Open::Chunk { start, chars } => {
                arena.merge_evidence(&mut self.open_evidence, &added, self.window.cancelled)?;
                self.open = Open::Chunk {
                    start,
                    chars: chars + delta,
                };
            }
            Open::Tail { at, start, chars } if chars + delta > self.limit(arena, at, index) => {
                self.close_tail(at, start..index, chars, out);
                self.start_chunk(arena, index, item, &added)?;
            }
            Open::Tail { at, start, chars } => {
                arena.merge_evidence(&mut self.open_evidence, &added, self.window.cancelled)?;
                self.open = Open::Tail {
                    at,
                    start,
                    chars: chars + delta,
                };
            }
        }
        Ok(())
    }

    fn oversized(
        &mut self,
        arena: &mut FragmentArena,
        index: usize,
        out: &mut Vec<Planned>,
    ) -> Result<(), DependencyError> {
        if let Open::Chunk { start, chars } = self.open
            && index > start
        {
            let base = &arena.splits[self.split].contexts[usize::from(start > 0)];
            let pending = (chars - arena.evidence_chars(&self.open_evidence)?)
                - (base.chars - arena.evidence_chars(&base.evidence)?);
            let at = out.len();
            let limit = self.limit(arena, at, index);
            let mut trial = Vec::new();
            // Reserve only neighbouring JSON here. The trial may share all of
            // its descriptors; the exact union decides whether the join fits.
            let head = limit.saturating_sub(pending).max(1);
            arena.plan_in(
                self.single(arena, index, at),
                self.catalog,
                self.item_window(arena, index)
                    .with_head(head)
                    .with_continuation(continues_chars(&self.pointer, index)),
                &mut trial,
            )?;
            let extra = arena.evidence_delta(
                &trial[0].0.evidence,
                &self.open_evidence,
                self.window.cancelled,
            )?;
            if trial[0].1 + pending + extra <= limit {
                out.extend(trial);
                mark_continuations(&self.pointer, at, index, out);
                self.join_before(arena, at, start..index, pending + extra, out)?;
            } else {
                self.flush(start, index, chars, out);
                let at = out.len();
                arena.plan_in(
                    self.single(arena, index, at),
                    self.catalog,
                    self.item_window(arena, index)
                        .with_head(self.limit(arena, at, index))
                        .with_continuation(continues_chars(&self.pointer, index)),
                    out,
                )?;
                mark_continuations(&self.pointer, at, index, out);
            }
        } else {
            self.close(arena, index, out);
            let at = out.len();
            arena.plan_in(
                self.single(arena, index, at),
                self.catalog,
                self.item_window(arena, index)
                    .with_head(self.limit(arena, at, index))
                    .with_continuation(continues_chars(&self.pointer, index)),
                out,
            )?;
            mark_continuations(&self.pointer, at, index, out);
        }
        let last = out.len() - 1;
        self.open = Open::Tail {
            at: last,
            start: index + 1,
            chars: out[last].1,
        };
        self.open_evidence = out[last].0.evidence.clone();
        Ok(())
    }

    /// The window a single item plans in: the tail room only when it ends
    /// the split.
    fn item_window(&self, arena: &FragmentArena, index: usize) -> Window<'_> {
        self.window
            .for_item(index, arena.splits[self.split].items.len())
    }

    fn single(&self, arena: &FragmentArena, index: usize, at: usize) -> Value {
        let split = &arena.splits[self.split];
        let mut single = split.contexts[usize::from(at != self.first)].value.clone();
        if let Some(slot) = single.pointer_mut(&split.pointer) {
            *slot = Value::Array(vec![split.items[index].clone()]);
        }
        single
    }

    pub(super) fn close(&mut self, arena: &FragmentArena, end: usize, out: &mut Vec<Planned>) {
        match self.open {
            Open::Chunk { start, chars } => self.flush(start, end, chars, out),
            Open::Tail { at, start, chars } => self.close_tail(at, start..end, chars, out),
        }
        let context = &arena.splits[self.split].contexts[usize::from(end > 0)];
        self.open = Open::Chunk {
            start: end,
            chars: context.chars,
        };
        self.open_evidence = context.evidence.clone();
    }

    fn flush(&self, start: usize, end: usize, chars: usize, out: &mut Vec<Planned>) {
        if end <= start {
            return;
        }
        let chunk = FragmentKind::Chunk {
            split: self.split,
            start,
            end,
        };
        let element = (out.len() > self.first)
            .then(|| self.enclosing.clone())
            .flatten();
        let (kind, size) = match element {
            Some((array, index)) => {
                let size = chars - 1 + continues_chars(&array, index);
                (
                    FragmentKind::Continues {
                        inner: Box::new(chunk),
                        array,
                        index,
                    },
                    size,
                )
            }
            None => (chunk, chars - 1),
        };
        out.push((
            Fragment {
                kind,
                evidence: self.open_evidence.clone(),
            },
            size,
        ));
    }

    fn join_before(
        &self,
        arena: &FragmentArena,
        at: usize,
        before: Range<usize>,
        chars: usize,
        out: &mut [Planned],
    ) -> Result<(), DependencyError> {
        let (fragment, size) = &mut out[at];
        let inner = std::mem::replace(&mut fragment.kind, FragmentKind::Whole(0));
        fragment.kind = FragmentKind::Joined {
            inner: Box::new(inner),
            split: self.split,
            before,
            after: 0..0,
        };
        arena.merge_evidence(
            &mut fragment.evidence,
            &self.open_evidence,
            self.window.cancelled,
        )?;
        *size += chars;
        Ok(())
    }

    fn close_tail(&self, at: usize, after: Range<usize>, chars: usize, out: &mut [Planned]) {
        if after.is_empty() {
            return;
        }
        let (fragment, size) = &mut out[at];
        match &mut fragment.kind {
            FragmentKind::Joined {
                split, after: slot, ..
            } if *split == self.split => *slot = after,
            _ => {
                let inner = std::mem::replace(&mut fragment.kind, FragmentKind::Whole(0));
                fragment.kind = FragmentKind::Joined {
                    inner: Box::new(inner),
                    split: self.split,
                    before: 0..0,
                    after,
                };
            }
        }
        fragment.evidence = self.open_evidence.clone();
        *size = chars;
    }
}
