use super::{
    Fragment, FragmentArena, Planned, continues_chars, empty_secondary_lists, mark_continuations,
};
use serde_json::Value;
use std::ops::Range;

/// The items of a split not yet in a fragment.
#[derive(Clone, Copy)]
pub(super) enum Open {
    /// Items from `start` form a new slice of `chars` (skeleton included).
    Chunk { start: usize, chars: usize },
    /// Items from `start` join fragment `at`, the last part of a recursed
    /// element, now `chars`.
    Tail {
        at: usize,
        start: usize,
        chars: usize,
    },
}

/// Greedy slicing of one split's items: the row's first fragment is bounded
/// by `head`, every later one by `budget`.
pub(super) struct Slicer {
    pub(super) split: usize,
    pub(super) enclosing: Option<(String, usize)>,
    /// The paged array's pointer.
    pub(super) pointer: String,
    pub(super) first: usize,
    pub(super) head: usize,
    pub(super) budget: usize,
    pub(super) base: usize,
    pub(super) open: Open,
}

impl Slicer {
    /// The bound of the fragment at position `at` of the plan.
    fn limit(&self, at: usize) -> usize {
        if at == self.first {
            self.head
        } else {
            self.budget
        }
    }

    /// Place item `index` of `item` serialized chars (its comma included).
    pub(super) fn push(
        &mut self,
        arena: &mut FragmentArena,
        index: usize,
        item: usize,
        out: &mut Vec<Planned>,
    ) {
        if self.base + item > self.limit(out.len()) {
            self.oversized(arena, index, out);
            return;
        }
        self.open = match self.open {
            Open::Chunk { start, chars }
                if index > start && chars + item > self.limit(out.len()) =>
            {
                self.flush(start, index, chars, out);
                Open::Chunk {
                    start: index,
                    chars: self.base + item,
                }
            }
            Open::Chunk { start, chars } => Open::Chunk {
                start,
                chars: chars + item,
            },
            Open::Tail { at, start, chars } if chars + item > self.limit(at) => {
                self.close_tail(at, start..index, chars, out);
                Open::Chunk {
                    start: index,
                    chars: self.base + item,
                }
            }
            Open::Tail { at, start, chars } => Open::Tail {
                at,
                start,
                chars: chars + item,
            },
        };
    }

    /// Item `index` alone exceeds a fragment: it recurses on a one-item row.
    /// The open slice before it joins the element's first fragment when both
    /// fit the bound, and the items after it join its last fragment while
    /// they fit, so a seam never leaves a part short of its page.
    fn oversized(&mut self, arena: &mut FragmentArena, index: usize, out: &mut Vec<Planned>) {
        if let Open::Chunk { start, chars } = self.open
            && index > start
        {
            let pending = chars - self.base;
            let at = out.len();
            let limit = self.limit(at);
            let mut trial = Vec::new();
            let head = limit.saturating_sub(pending).max(1);
            arena.plan(self.single(arena, index, at), head, self.budget, &mut trial);
            if trial[0].1 + pending <= limit {
                out.extend(trial);
                mark_continuations(&self.pointer, at, index, out);
                self.join_before(at, start..index, pending, out);
            } else {
                // The trial's arena entries are never built.
                self.flush(start, index, chars, out);
                let at = out.len();
                arena.plan(
                    self.single(arena, index, at),
                    self.limit(at),
                    self.budget,
                    out,
                );
                mark_continuations(&self.pointer, at, index, out);
            }
        } else {
            self.close(index, out);
            let at = out.len();
            arena.plan(
                self.single(arena, index, at),
                self.limit(at),
                self.budget,
                out,
            );
            mark_continuations(&self.pointer, at, index, out);
        }
        let last = out.len() - 1;
        self.open = Open::Tail {
            at: last,
            start: index + 1,
            chars: out[last].1,
        };
    }

    /// Item `index` as a one-item row planned at position `at`; after the
    /// split's first fragment it carries the secondary lists emptied.
    fn single(&self, arena: &FragmentArena, index: usize, at: usize) -> Value {
        let row_split = &arena.splits[self.split];
        let mut single = row_split.skeleton.clone();
        if at != self.first {
            empty_secondary_lists(&mut single, &row_split.pointer);
        }
        if let Some(slot) = single.pointer_mut(&row_split.pointer) {
            *slot = Value::Array(vec![row_split.items[index].clone()]);
        }
        single
    }

    pub(super) fn close(&mut self, end: usize, out: &mut Vec<Planned>) {
        match self.open {
            Open::Chunk { start, chars } => self.flush(start, end, chars, out),
            Open::Tail { at, start, chars } => self.close_tail(at, start..end, chars, out),
        }
        self.open = Open::Chunk {
            start: end,
            chars: self.base,
        };
    }

    /// Push the slice of items `start..end`. A slice after the split's
    /// first part continues the element its array lies in, if any.
    fn flush(&self, start: usize, end: usize, chars: usize, out: &mut Vec<Planned>) {
        if end <= start {
            return;
        }
        let split = self.split;
        let chunk = Fragment::Chunk { split, start, end };
        let element = (out.len() > self.first)
            .then(|| self.enclosing.clone())
            .flatten();
        out.push(match element {
            Some((array, index)) => {
                let size = chars - 1 + continues_chars(&array, index);
                let inner = Box::new(chunk);
                (
                    Fragment::Continues {
                        inner,
                        array,
                        index,
                    },
                    size,
                )
            }
            None => (chunk, chars - 1),
        });
    }

    /// Items `before` precede the one element part of fragment `at`.
    fn join_before(&self, at: usize, before: Range<usize>, chars: usize, out: &mut [Planned]) {
        let (fragment, size) = &mut out[at];
        let inner = std::mem::replace(fragment, Fragment::Whole(0));
        *fragment = Fragment::Joined {
            inner: Box::new(inner),
            split: self.split,
            before,
            after: 0..0,
        };
        *size += chars;
    }

    /// Items `after` follow the one element part of fragment `at`, now `chars`.
    fn close_tail(&self, at: usize, after: Range<usize>, chars: usize, out: &mut [Planned]) {
        if after.is_empty() {
            return;
        }
        let (fragment, size) = &mut out[at];
        match fragment {
            Fragment::Joined {
                split, after: slot, ..
            } if *split == self.split => *slot = after,
            _ => {
                let inner = std::mem::replace(fragment, Fragment::Whole(0));
                *fragment = Fragment::Joined {
                    inner: Box::new(inner),
                    split: self.split,
                    before: 0..0,
                    after,
                };
            }
        }
        *size = chars;
    }
}
