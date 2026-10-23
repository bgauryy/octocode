//! Metavariable bindings with an undo log.

use std::collections::HashMap;

use super::{RawRange, SECONDARY_CAPTURE};

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
    ranges: HashMap<String, Vec<RawRange>>,
    undo: Vec<Undo>,
}

enum Undo {
    /// `name` was newly bound: rollback removes it.
    Insert(String),
    /// `name` was rebound (`capture_replace`): rollback restores the old value.
    Replace(String, Vec<String>, Vec<RawRange>),
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
        while self.undo.len() > checkpoint.0 {
            match self.undo.pop() {
                Some(Undo::Insert(name)) => {
                    self.values.remove(&name);
                    self.ranges.remove(&name);
                }
                Some(Undo::Replace(name, values, ranges)) => {
                    self.values.insert(name.clone(), values);
                    self.ranges.insert(name, ranges);
                }
                None => break,
            }
        }
    }

    fn insert(&mut self, name: &str, values: Vec<String>, ranges: Vec<RawRange>) {
        self.values.insert(name.to_owned(), values);
        self.ranges.insert(name.to_owned(), ranges);
        self.undo.push(Undo::Insert(name.to_owned()));
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
                self.insert(name, vec![text.to_owned()], vec![range]);
                true
            }
        }
    }

    /// Bookkeeping capture for relational rules (`has`/`inside` record the
    /// related node as "secondary"). Unlike user metavars, it carries no
    /// backreference semantics: nested relations each match a different node,
    /// so consistency-checking it (capture_one) rejects valid matches — the
    /// nearest relation simply wins.
    pub(in crate::structural::octo) fn capture_replace(
        &mut self,
        name: &str,
        text: String,
        range: RawRange,
    ) {
        let old_values = self.values.insert(name.to_owned(), vec![text]);
        let old_ranges = self.ranges.insert(name.to_owned(), vec![range]);
        self.undo.push(match old_values {
            Some(values) => Undo::Replace(name.to_owned(), values, old_ranges.unwrap_or_default()),
            None => Undo::Insert(name.to_owned()),
        });
    }

    /// Bind `name` to a node sequence, or check it equals the existing binding.
    pub(in crate::structural::octo) fn capture_many<'s>(
        &mut self,
        name: &str,
        texts: impl ExactSizeIterator<Item = &'s str> + Clone,
        ranges: impl FnOnce() -> Vec<RawRange>,
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

    pub(in crate::structural::octo) fn into_maps(
        mut self,
    ) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<RawRange>>) {
        self.values.remove(SECONDARY_CAPTURE);
        self.ranges.remove(SECONDARY_CAPTURE);
        (self.values, self.ranges)
    }

    #[cfg(test)]
    pub(in crate::structural::octo) fn undo_len(&self) -> usize {
        self.undo.len()
    }
}
