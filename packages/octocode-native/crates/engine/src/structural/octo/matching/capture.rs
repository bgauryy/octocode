//! Metavariable bindings with an undo log.

use std::collections::HashMap;

use super::{CapturedRanges, ListedRange, RawRange};

/// Metavariable bindings for one candidate match, with an undo log.
///
/// Branching matchers (`has`/`inside`/`all`/`any`/`not`, `$$$` split points,
/// child lists) do not clone the environment per branch. Instead each
/// branch takes a [`checkpoint`](Self::checkpoint) and, on failure,
/// [`rollback`](Self::rollback)s to it: bindings are append-only per branch,
/// so undoing them is O(bindings added since the checkpoint) and a successful
/// branch costs nothing.
#[derive(Default)]
pub(in crate::structural::octo) struct CaptureEnv {
    values: HashMap<String, Vec<String>>,
    /// Each name's spans, with the index of the value each one is: a `$$$`
    /// capture lists no span for its punctuation ([`super::listed_ranges`]).
    ranges: CapturedRanges,
    /// The names bound since the start, in binding order: rollback removes
    /// them. A bound name is never rebound.
    undo: Vec<String>,
}

/// Position in the undo log returned by [`CaptureEnv::checkpoint`].
#[derive(Clone, Copy)]
pub(in crate::structural::octo) struct Checkpoint(usize);

impl CaptureEnv {
    pub(in crate::structural::octo) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.undo.len())
    }

    /// Undo every binding made since `checkpoint`.
    pub(in crate::structural::octo) fn rollback(&mut self, checkpoint: Checkpoint) {
        for name in self.undo.drain(checkpoint.0.min(self.undo.len())..) {
            self.values.remove(&name);
            self.ranges.remove(&name);
        }
    }

    fn insert(&mut self, name: &str, values: Vec<String>, ranges: Vec<ListedRange>) {
        self.values.insert(name.to_owned(), values);
        self.ranges.insert(name.to_owned(), ranges);
        self.undo.push(name.to_owned());
    }

    /// Bind `name` to `text`, or check it equals the existing binding
    /// (backreference). Allocates only when binding a new name.
    pub(in crate::structural::octo) fn capture_one(
        &mut self,
        name: &str,
        text: &str,
        range: RawRange,
    ) -> bool {
        match self.values.get(name) {
            Some(existing) => existing.len() == 1 && existing[0] == text,
            None => {
                self.insert(name, vec![text.to_owned()], vec![(0, range)]);
                true
            }
        }
    }

    /// Bind `name` to a node sequence, or check it equals the existing binding.
    /// `ranges` lists the sequence's spans, each with its value's index.
    pub(in crate::structural::octo) fn capture_many<'s>(
        &mut self,
        name: &str,
        texts: impl ExactSizeIterator<Item = &'s str> + Clone,
        ranges: impl FnOnce() -> Vec<ListedRange>,
    ) -> bool {
        match self.values.get(name) {
            Some(existing) => {
                existing.len() == texts.len()
                    && existing
                        .iter()
                        .zip(texts)
                        .all(|(left, right)| left == right)
            }
            None => {
                let texts = texts.map(str::to_owned).collect();
                self.insert(name, texts, ranges());
                true
            }
        }
    }

    /// The trees the captured nodes sit in ([`super::super::source_context::tree_id`]).
    pub(in crate::structural::octo) fn trees(&self) -> impl Iterator<Item = usize> + '_ {
        self.ranges.values().flatten().map(|(_, range)| range.6)
    }

    pub(in crate::structural::octo) fn into_maps(
        self,
    ) -> (HashMap<String, Vec<String>>, CapturedRanges) {
        (self.values, self.ranges)
    }

    #[cfg(test)]
    pub(in crate::structural::octo) fn undo_len(&self) -> usize {
        self.undo.len()
    }
}
