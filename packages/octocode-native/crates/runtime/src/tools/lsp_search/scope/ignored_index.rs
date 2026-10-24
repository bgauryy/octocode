//! The scope's ignored-file index: the family files ignore rules keep out of
//! every lexical scan, listed once per workspace snapshot.
//!
//! Finding which ignored files spell a name means walking the ignored tree
//! (a gitignored `campaigns/` or corpus can hold tens of thousands of
//! entries), whatever the name. The walk is name-free, so it runs once per
//! scope in the background, and each request then reads only the listed
//! files for its name. The index holds while its workspace snapshot does
//! (`snapshot`). A walk past its bounds keeps what it listed, says it is
//! incomplete, and is rebuilt after [`INCOMPLETE_TTL`]. Nested repositories
//! (directories with their own `.git`) are other repositories: the walk
//! does not enter them and names them.
use super::*;
use crate::tools::lsp_search::words::file_spells;
use snapshot::{Snapshot, build};
use std::collections::HashSet;

mod snapshot;
/// How long an incomplete index (walk past its bounds) is reused before
/// the walk runs again: its stamps do not cover the tree it did not reach.
const INCOMPLETE_TTL: Duration = Duration::from_secs(600);
/// How often a waiting request looks for the index a background walk is
/// building.
const WAIT_POLL: Duration = Duration::from_millis(20);

/// Built indexes by scope key (`Scope::index_key`).
static INDEXES: octocode_io::memo::Memo<String, IgnoredIndex> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 16,
        max_weight: 16,
        keep_newest: false,
    });

/// Scope keys whose index a background walk is building.
static BUILDING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

#[derive(Debug, Default)]
pub(super) struct IgnoredIndex {
    /// Canonical family files the honored walk leaves out, sorted.
    pub(super) files: Vec<String>,
    /// Nested repositories the walk did not enter, root-relative, sorted.
    pub(super) nested: Vec<String>,
    /// Both walks finished within their bounds.
    pub(super) complete: bool,
    /// The workspace snapshot the walk saw.
    snapshot: Snapshot,
}

impl IgnoredIndex {
    /// Whether the snapshot the index was built from still holds.
    fn holds(&self, stopped: &(dyn Fn() -> bool + Sync)) -> bool {
        self.snapshot.holds(self, stopped)
    }

    /// The listed files that spell `symbol` as a word, and whether every
    /// listed file was read within `budget`.
    pub(super) fn mentions(
        &self,
        symbol: &str,
        budget: Duration,
        stopped: &(dyn Fn() -> bool + Sync),
    ) -> (BTreeSet<String>, bool) {
        let started = Instant::now();
        let mut found = BTreeSet::new();
        for file in &self.files {
            if stopped() || started.elapsed() > budget {
                return (found, false);
            }
            // An unreadable file is not proven free of the name.
            if file_spells(Path::new(file), symbol.as_bytes()).unwrap_or(true) {
                found.insert(file.clone());
            }
        }
        (found, true)
    }
}

/// Clears the scope's building mark when its walk ends, however it ends.
struct Building(String);

impl Drop for Building {
    fn drop(&mut self) {
        if let Some(keys) = BUILDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
        {
            keys.remove(&self.0);
        }
    }
}

/// Build the index of `key` (`root`, family `include`, read `policy`) on a
/// background thread, unless one is held or building.
pub(super) fn start(key: String, root: PathBuf, include: Vec<String>, policy: PathPolicy) {
    if INDEXES.get(&key).is_some() {
        return;
    }
    {
        let mut keys = BUILDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !keys.get_or_insert_with(HashSet::new).insert(key.clone()) {
            return;
        }
    }
    let mark = Building(key);
    // A thread that cannot start drops its mark: the next request retries.
    let _ = std::thread::Builder::new()
        .name("octocode-ignored-index".into())
        .spawn(move || {
            let index = build(&root, &include, &policy, &|| false);
            let ttl = (!index.complete).then_some(INCOMPLETE_TTL);
            INDEXES.insert(mark.0.clone(), index, 1, ttl);
            drop(mark);
        });
}

fn building(key: &str) -> bool {
    BUILDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .is_some_and(|keys| keys.contains(key))
}

impl Scope {
    /// This scope's index: a held one whose snapshot still holds, else the
    /// one a background walk builds within `wait`; `None` past it.
    pub(super) async fn ignored_index(
        &self,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
        wait: Duration,
    ) -> Result<Option<Arc<IgnoredIndex>>, LspFailure> {
        let key = self.index_key(policy);
        if let Some(held) = INDEXES.get(&key).map(|hit| hit.value) {
            // An incomplete index expires by age; a complete one by its stamps.
            let checked = Arc::clone(&held);
            let holds = !held.complete
                || blocking_cancellable(cancel, move |stopped| checked.holds(stopped))
                    .await?
                    .unwrap_or(false);
            if holds {
                return Ok(Some(held));
            }
            INDEXES.remove(&key);
        }
        self.warm_ignored_index(policy);
        let deadline = Instant::now() + wait;
        loop {
            if let Some(hit) = INDEXES.get(&key) {
                return Ok(Some(hit.value));
            }
            if !building(&key) || Instant::now() >= deadline {
                return Ok(None);
            }
            cancel.check().map_err(LspFailure::cancelled)?;
            tokio::time::sleep(WAIT_POLL).await;
        }
    }
}

#[cfg(test)]
mod tests;
