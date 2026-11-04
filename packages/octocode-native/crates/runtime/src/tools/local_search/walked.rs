//! What a fresh text-search walk searched, recorded as it walks, so a stored
//! scan is served only while none of it changed (see [`super::manifest`]).
//! Every searched file and every walked directory is stamped: a new file
//! changes its directory, and an existing file that gains a match changes
//! its own stamp. With ignore rules on, each walked directory's ignore files,
//! the rules outside the walk and the repository markers are watched too.
use crate::policy::discovery::WalkAdmission;
use crate::policy::path::PathPolicy;
use crate::tools::page_memo::DirectoryInputs;
use octocode_engine::search::text_search::TextSearchPathFilter;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::SystemTime,
};

/// What one walk searched and what its ignore rules depend on.
pub struct Walked {
    pub(super) root: PathBuf,
    /// Searched files, walked directories, and their ignore files.
    pub(super) entries: Vec<PathBuf>,
    /// How many of `entries`, from the first, are the searched files.
    searched: usize,
    /// Ignore inputs outside the walk.
    pub(super) rules: Vec<PathBuf>,
    /// Repository markers whose presence decides the rules.
    pub(super) markers: Vec<PathBuf>,
    /// An ignore dependency no stamp can watch (a link, an unreadable or
    /// withheld entry): the scan is not stored.
    pub(super) untracked: bool,
    /// The recorded paths alone outweighed what a stored scan may keep, so
    /// the walk stopped keeping them: the scan is not stored.
    pub(super) oversize: bool,
    /// When the walk started: an entry written within the settle window
    /// before it could race the walk's reads, so the scan is not stored.
    pub(super) started: SystemTime,
}

impl Walked {
    /// Heap bytes of the recorded paths, toward a stored scan's weight.
    pub(super) fn weight(&self) -> usize {
        self.entries
            .iter()
            .chain(&self.rules)
            .chain(&self.markers)
            .map(|path| path_weight(path))
            .sum()
    }

    /// A walk of `entries` under `root` with no ignore inputs, started past
    /// the settle window of every write so far.
    #[cfg(test)]
    pub(super) fn settled(root: PathBuf, entries: Vec<PathBuf>) -> Self {
        Self {
            root,
            entries,
            searched: 0,
            rules: Vec::new(),
            markers: Vec::new(),
            untracked: false,
            oversize: false,
            started: SystemTime::now()
                + octocode_engine::stamp::SourceStamp::SETTLE
                + std::time::Duration::from_secs(1),
        }
    }

    /// The files the walk admitted to search, when it kept a record fit to
    /// store; otherwise another search of them walks again.
    pub(super) fn searched_files(&self) -> Option<&[PathBuf]> {
        (!self.oversize && !self.untracked).then(|| &self.entries[..self.searched])
    }

    /// The proof a stored scan of this walk keeps; `None` when an entry is
    /// gone or was written within the settle window before the walk.
    pub(super) fn proof(&self) -> Option<octocode_engine::stamp::Proof> {
        crate::tools::page_memo::Watch {
            root: &self.root,
            sources: &self.entries,
            rules: &self.rules,
            markers: &self.markers,
        }
        .capture(octocode_engine::stamp::Link::Follow, Some(self.started))
    }
}

/// Heap bytes one recorded path keeps.
fn path_weight(path: &Path) -> usize {
    std::mem::size_of::<PathBuf>() + path.as_os_str().len()
}

/// Shards of a walk's record. Each walk thread records into its own shard
/// ([`Recorder::shard`]), so threads seldom wait on one another's records.
const SHARDS: usize = 16;

/// The walk's path filter: [`WalkAdmission`]'s verdicts, recording each
/// admitted entry.
pub(super) struct Recorder {
    walk: Arc<WalkAdmission<'static>>,
    policy: PathPolicy,
    root: PathBuf,
    ignore_rules: bool,
    shards: [Mutex<Seen>; SHARDS],
    weight: Weight,
    started: SystemTime,
}

/// The heap bytes ([`path_weight`]) every shard's entries weigh together.
struct Weight {
    bytes: AtomicUsize,
    /// Bytes the recorded entries may weigh: past it no stored scan could
    /// keep them, so the recorder keeps none.
    max: usize,
    oversize: AtomicBool,
}

#[derive(Default)]
struct Seen {
    /// Files the walk admitted to search, apart from what it only watches.
    searched: Vec<PathBuf>,
    entries: Vec<PathBuf>,
    rules: BTreeSet<PathBuf>,
    untracked: bool,
}

impl Seen {
    /// Record a walked directory's ignore inputs, read before the lock.
    fn observe(&mut self, inputs: DirectoryInputs, weight: &Weight) {
        self.rules.extend(inputs.external);
        self.untracked |= inputs.untracked;
        for ignore in inputs.files {
            self.record(ignore, weight);
        }
    }

    /// Record `path`, or drop every entry once all shards' entries
    /// outweigh the maximum.
    fn record(&mut self, path: PathBuf, weight: &Weight) {
        if weight.keeps(&path) {
            self.entries.push(path);
        } else {
            self.drop_entries();
        }
    }

    /// [`Self::record`] of a file the walk admitted to search.
    fn record_searched(&mut self, path: PathBuf, weight: &Weight) {
        if weight.keeps(&path) {
            self.searched.push(path);
        } else {
            self.drop_entries();
        }
    }

    fn drop_entries(&mut self) {
        self.entries = Vec::new();
        self.searched = Vec::new();
    }
}

impl Weight {
    /// Whether `path` fits beside what all shards keep; the first that does
    /// not marks the walk oversize.
    fn keeps(&self, path: &Path) -> bool {
        if self.oversize.load(Ordering::Relaxed) {
            return false;
        }
        let added = path_weight(path);
        let kept = self
            .bytes
            .fetch_add(added, Ordering::Relaxed)
            .saturating_add(added)
            <= self.max;
        if !kept {
            self.oversize.store(true, Ordering::Relaxed);
        }
        kept
    }
}

impl Recorder {
    /// A recorder of `walk` over `root`; `ignore_rules` when the walk
    /// applies ignore files. It keeps at most `max_weight` bytes of entries.
    pub(super) fn new(
        walk: Arc<WalkAdmission<'static>>,
        policy: PathPolicy,
        root: PathBuf,
        ignore_rules: bool,
        max_weight: usize,
    ) -> Self {
        let rules = if ignore_rules {
            octocode_engine::search::walk::external_ignore_inputs(&root)
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        let recorder = Self {
            walk,
            policy,
            root,
            ignore_rules,
            shards: std::array::from_fn(|_| Mutex::default()),
            weight: Weight {
                bytes: AtomicUsize::new(0),
                max: max_weight,
                oversize: AtomicBool::new(false),
            },
            started: octocode_engine::stamp::settle_clock(),
        };
        let mut seen = recorder.shard();
        seen.rules = rules;
        // The root's own ignore files decide the walk whether or not the
        // walk reports the root as an entry.
        if ignore_rules && recorder.root.is_dir() {
            let inputs = recorder.directory_inputs(&recorder.root);
            seen.observe(inputs, &recorder.weight);
        }
        drop(seen);
        recorder
    }

    /// What the walk recorded: every shard's record, in shard order.
    pub(super) fn walked(&self) -> Walked {
        let oversize = self.weight.oversize.load(Ordering::Relaxed);
        let mut seen = Seen::default();
        for shard in &self.shards {
            let shard = std::mem::take(&mut *lock(shard));
            if !oversize {
                seen.searched.extend(shard.searched);
                seen.entries.extend(shard.entries);
            }
            seen.rules.extend(shard.rules);
            seen.untracked |= shard.untracked;
        }
        let searched = seen.searched.len();
        let mut entries = seen.searched;
        entries.append(&mut seen.entries);
        Walked {
            markers: if self.ignore_rules {
                crate::tools::page_memo::repository_markers(&self.root)
            } else {
                Vec::new()
            },
            root: self.root.clone(),
            entries,
            searched,
            rules: seen.rules.into_iter().collect(),
            untracked: seen.untracked,
            oversize,
            started: self.started,
        }
    }

    /// The calling thread's shard of the record: threads are numbered as
    /// they first record, so concurrent walk threads take distinct shards.
    fn shard(&self) -> MutexGuard<'_, Seen> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        thread_local! {
            static SLOT: usize = NEXT.fetch_add(1, Ordering::Relaxed);
        }
        lock(&self.shards[SLOT.with(|slot| *slot) % SHARDS])
    }

    /// A walked directory's ignore inputs
    /// ([`crate::tools::page_memo::directory_ignore_inputs`]).
    fn directory_inputs(&self, dir: &Path) -> DirectoryInputs {
        crate::tools::page_memo::directory_ignore_inputs(dir, &self.root, |ignore| {
            self.policy.validate_read(ignore).is_ok()
        })
    }

    /// Record `path` when the walk admitted it, as a file it goes on to
    /// search when `file`. A directory's ignore inputs are read before the
    /// lock is taken, so no walk thread waits on another thread's file-system
    /// calls.
    fn admitted(&self, path: &Path, is_dir: bool, file: bool, allowed: bool) -> bool {
        if allowed {
            let inputs = (is_dir && self.ignore_rules && path != self.root)
                .then(|| self.directory_inputs(path));
            let entry = path.to_path_buf();
            let mut seen = self.shard();
            if file {
                seen.record_searched(entry, &self.weight);
            } else {
                seen.record(entry, &self.weight);
            }
            if let Some(inputs) = inputs {
                seen.observe(inputs, &self.weight);
            }
        }
        allowed
    }
}

/// A shard of the record; a poisoned one is still read.
fn lock(shard: &Mutex<Seen>) -> MutexGuard<'_, Seen> {
    shard
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl TextSearchPathFilter for Recorder {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        self.admitted(path, is_dir, !is_dir, self.walk.allows(path, is_dir))
    }

    fn allows_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        let is_dir = file_type.is_some_and(|kind| kind.is_dir());
        let file = file_type.is_some_and(|kind| kind.is_file());
        self.admitted(path, is_dir, file, self.walk.allows_entry(path, file_type))
    }

    fn skipped_hidden(&self, path: &Path, is_dir: bool) {
        self.walk.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &Path, is_dir: bool) {
        self.walk.skipped_ignored(path, is_dir);
    }

    fn skipped_symlink(&self, path: &Path) {
        self.walk.skipped_symlink(path);
    }
}

#[cfg(test)]
mod tests;
