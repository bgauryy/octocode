//! What concurrent scans of one scope share: the walk, recorded once and
//! replayed through each scan's own `admit`, and the syntax tree of each
//! source they parse.
//!
//! A replayed walk calls the scan's `admit` with the same entries, in the
//! same order, with the same entry types the walker gave, and records the
//! same errors and left-out entries up to the same cut, so its candidates,
//! counts and everything its `admit` records are what a walk of its own
//! would give over the same entries. Whether those entries are still the
//! scope's is the caller's to prove ([`WalkRecord::dirs`]).
//!
//! A shared tree is the parse of the same text with the same grammar, which
//! is what a parse of its own would give: the share hands it only to a scan
//! matching the same text.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use super::walk::Cause;

/// One step of a walk, in walk order.
pub(super) enum Step {
    /// An entry the walker gave, with its type.
    Entry(PathBuf, Option<std::fs::FileType>),
    /// An entry the walker could not read.
    Error { denied: bool, message: String },
    /// An entry the walk's own rules left out, as the walk shows it.
    LeftOut(Cause, String),
}

/// A whole walk of one directory ([`Walk::Record`]): every step its walker
/// took, to its end.
pub struct WalkRecord {
    pub(super) steps: Vec<Step>,
    /// The directories the walker read, the root first.
    pub(super) dirs: Vec<PathBuf>,
}

impl WalkRecord {
    /// The directories the walk read, the root first: an entry added,
    /// removed or renamed in one changes its stamp, and their ignore files
    /// decide what the walk leaves out.
    #[must_use]
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// About the heap bytes this record holds.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        let path = |path: &Path| 32 + path.as_os_str().len();
        self.steps
            .iter()
            .map(|step| {
                std::mem::size_of::<Step>()
                    + match step {
                        Step::Entry(entry, _) => path(entry),
                        Step::Error { message, .. } | Step::LeftOut(_, message) => message.len(),
                    }
            })
            .sum::<usize>()
            + self.dirs.iter().map(|dir| path(dir)).sum::<usize>()
    }
}

/// How a scan walks its scope.
#[derive(Clone, Copy, Default)]
pub enum Walk<'a> {
    /// It walks.
    #[default]
    Fresh,
    /// It walks a directory and hands its record to the callback once the
    /// walk is done, before any file is read: `None` when the walk was cut
    /// before the walker's end (by `max_files`), which no other scan
    /// replays. A walk that fails calls nothing.
    Record(&'a (dyn Fn(Option<WalkRecord>) + Sync)),
    /// It replays a whole walk of the same directory, globs, default
    /// excludes, ignore policy, global ignore rules and depth.
    Replay(&'a WalkRecord),
}

/// The trees of the sources concurrent scans of one scope parse: while two
/// scans or more take part ([`TreeShare::join`]), each source is parsed
/// once, and every scan matching the same text with the same grammar reads
/// that parse; a scan that finds a parse under way waits for it. A lone
/// scan keeps no tree. Trees are kept up to a cap of estimated parse memory
/// ([`fn@crate::structural::parse_cost`]); a parse past it is handed only to
/// the scans waiting for it. Everything is freed with the share.
pub struct TreeShare {
    trees: Mutex<HashMap<(PathBuf, tree_sitter::Language), Arc<Parse>>>,
    held: Mutex<usize>,
    cap: usize,
    /// The scans taking part now.
    scans: AtomicUsize,
    /// Parses this share made.
    parses: AtomicUsize,
}

/// One source's parse: under way until it lands with its tree (or none, when
/// the parse failed or its scan unwound).
struct Parse {
    text: Arc<str>,
    tree: Mutex<Parsed>,
    landed: Condvar,
}

/// Where a parse stands.
enum Parsed {
    UnderWay,
    /// It landed with its tree, or none when it failed.
    Landed(Option<tree_sitter::Tree>),
}

/// A scan's part in a [`TreeShare`], given up when dropped.
pub struct Joined<'a>(&'a TreeShare);

impl Drop for Joined<'_> {
    fn drop(&mut self) {
        self.0.scans.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How often a scan waiting for another's parse looks at its own stop.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "how often a waiter reads its own stop, not a work budget: no limit or deadline derives it"
    )
)]
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(10);

impl Default for TreeShare {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeShare {
    /// A share keeping trees up to a sixteenth of the process budget
    /// ([`crate::memory::MEMORY`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_cap(crate::memory::MEMORY.limit() / 16)
    }

    pub(crate) fn with_cap(cap: usize) -> Self {
        Self {
            trees: Mutex::new(HashMap::new()),
            held: Mutex::new(0),
            cap,
            scans: AtomicUsize::new(0),
            parses: AtomicUsize::new(0),
        }
    }

    /// Take part in this share for as long as the returned hold lives: a
    /// tree parsed while another scan takes part is kept for the others.
    #[must_use]
    pub fn join(&self) -> Joined<'_> {
        self.scans.fetch_add(1, Ordering::SeqCst);
        Joined(self)
    }

    /// The parse of `text`, the source at `path`, by `language`: the one
    /// this share holds or another scan is making of the same text, else
    /// `parse`'s, which the share keeps while another scan takes part and
    /// it has room. `stop` ends a wait for another scan's parse, and this
    /// scan parses for itself.
    pub(in crate::structural) fn tree<E>(
        &self,
        (path, language, text): (&Path, &tree_sitter::Language, &str),
        stop: &dyn Fn() -> bool,
        parse: impl FnOnce() -> Result<tree_sitter::Tree, E>,
    ) -> Result<tree_sitter::Tree, E> {
        let key = (path.to_path_buf(), language.clone());
        let joined = {
            let mut trees = self.trees.lock().unwrap_or_else(PoisonError::into_inner);
            match trees.get(&key) {
                Some(under_way) if *under_way.text == *text => Arc::clone(under_way),
                // None yet, or of another text (a write between two scans):
                // this scan's parse takes its place.
                _ => {
                    let lead = Arc::new(Parse {
                        text: Arc::from(text),
                        tree: Mutex::new(Parsed::UnderWay),
                        landed: Condvar::new(),
                    });
                    if let Some(replaced) = trees.insert(key.clone(), Arc::clone(&lead)) {
                        self.release(&replaced);
                    }
                    drop(trees);
                    return self.lead(&key, &lead, parse);
                }
            }
        };
        match joined.wait(stop) {
            Some(tree) => Ok(tree),
            None => self.counted(parse),
        }
    }

    /// Make the parse this scan leads and land it, kept when another scan
    /// takes part and there is room.
    fn lead<E>(
        &self,
        key: &(PathBuf, tree_sitter::Language),
        lead: &Arc<Parse>,
        parse: impl FnOnce() -> Result<tree_sitter::Tree, E>,
    ) -> Result<tree_sitter::Tree, E> {
        /// Lands the parse with nothing, and forgets it, when the scan
        /// unwinds before it lands.
        struct Landing<'a> {
            share: &'a TreeShare,
            key: &'a (PathBuf, tree_sitter::Language),
            lead: &'a Arc<Parse>,
            kept: bool,
            landed: bool,
        }
        impl Drop for Landing<'_> {
            fn drop(&mut self) {
                if !self.landed {
                    self.lead.land(None);
                }
                if !self.kept {
                    self.share.forget(self.key, self.lead);
                }
            }
        }
        let mut landing = Landing {
            share: self,
            key,
            lead,
            kept: false,
            landed: false,
        };
        let parsed = self.counted(parse);
        let tree = parsed.as_ref().ok().cloned();
        landing.kept = tree.is_some() && self.hold(lead);
        lead.land(tree);
        landing.landed = true;
        parsed
    }

    fn counted<E>(
        &self,
        parse: impl FnOnce() -> Result<tree_sitter::Tree, E>,
    ) -> Result<tree_sitter::Tree, E> {
        self.parses.fetch_add(1, Ordering::Relaxed);
        parse()
    }

    /// Drop `lead` from the share, when it still stands for its source.
    fn forget(&self, key: &(PathBuf, tree_sitter::Language), lead: &Arc<Parse>) {
        let mut trees = self.trees.lock().unwrap_or_else(PoisonError::into_inner);
        if trees.get(key).is_some_and(|held| Arc::ptr_eq(held, lead)) {
            trees.remove(key);
        }
    }

    /// Count `parse`'s tree as kept, when another scan takes part and the
    /// share has room for it.
    fn hold(&self, parse: &Parse) -> bool {
        if self.scans.load(Ordering::SeqCst) < 2 {
            return false;
        }
        let cost = crate::structural::parse_cost(parse.text.as_bytes());
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if held.saturating_add(cost) > self.cap {
            return false;
        }
        *held += cost;
        true
    }

    /// Count a replaced parse's tree as no longer kept.
    fn release(&self, parse: &Parse) {
        if matches!(
            *parse.tree.lock().unwrap_or_else(PoisonError::into_inner),
            Parsed::Landed(Some(_))
        ) {
            let cost = crate::structural::parse_cost(parse.text.as_bytes());
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            *held = held.saturating_sub(cost);
        }
    }

    /// The parses this share made, for tests.
    #[cfg(test)]
    pub(crate) fn parses(&self) -> usize {
        self.parses.load(Ordering::SeqCst)
    }

    /// The sources whose parse this share keeps or makes now, for tests.
    #[cfg(test)]
    pub(crate) fn kept(&self) -> usize {
        self.trees
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

impl Parse {
    fn land(&self, tree: Option<tree_sitter::Tree>) {
        *self.tree.lock().unwrap_or_else(PoisonError::into_inner) = Parsed::Landed(tree);
        self.landed.notify_all();
    }

    /// The tree once the parse lands; `None` when it landed without one, or
    /// `stop` says the waiter was stopped first.
    fn wait(&self, stop: &dyn Fn() -> bool) -> Option<tree_sitter::Tree> {
        let mut tree = self.tree.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Parsed::Landed(landed) = &*tree {
                return landed.clone();
            }
            if stop() {
                return None;
            }
            tree = self
                .landed
                .wait_timeout(tree, STOP_POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

#[cfg(test)]
mod tests;
