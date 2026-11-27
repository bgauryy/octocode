//! What a fresh text-search walk searched, recorded as it walks, so a stored
//! scan is served only while none of it changed (see [`super::manifest`]).
//! Every searched file and every walked directory is stamped: a new file
//! changes its directory, and an existing file that gains a match changes
//! its own stamp. With ignore rules on, each walked directory's ignore files,
//! the rules outside the walk and the repository markers are watched too.
use octocode_engine::search::text_search::TextSearchPathFilter;
use octocode_runtime_base::policy::discovery::WalkAdmission;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_tools_shared::page_memo::DirectoryInputs;
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
pub(super) struct Walked {
    pub(super) root: PathBuf,
    /// Searched files, walked directories, and their ignore files.
    pub(super) entries: Vec<PathBuf>,
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
            rules: Vec::new(),
            markers: Vec::new(),
            untracked: false,
            oversize: false,
            started: SystemTime::now()
                + octocode_engine::stamp::SourceStamp::SETTLE
                + std::time::Duration::from_secs(1),
        }
    }

    /// The proof a stored scan of this walk keeps; `None` when an entry is
    /// gone or was written within the settle window before the walk.
    pub(super) fn proof(&self) -> Option<octocode_engine::stamp::Proof> {
        octocode_tools_shared::page_memo::Watch {
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
    /// Directories whose ignore inputs were read (tests).
    #[cfg(test)]
    input_reads: AtomicUsize,
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
    /// The positions in `entries` of the walked directories whose ignore
    /// inputs are read when the record is taken ([`Recorder::walked`]).
    dirs: Vec<usize>,
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

    /// [`Self::record`] of a walked directory, whose ignore inputs are read
    /// when the record is taken.
    fn record_dir(&mut self, path: PathBuf, weight: &Weight) {
        if weight.keeps(&path) {
            self.dirs.push(self.entries.len());
            self.entries.push(path);
        } else {
            self.drop_entries();
        }
    }

    fn drop_entries(&mut self) {
        self.entries = Vec::new();
        self.searched = Vec::new();
        self.dirs = Vec::new();
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
            octocode_engine::search::walk::external_ignore_inputs_with(
                &root,
                policy.global_ignore(),
            )
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
            #[cfg(test)]
            input_reads: AtomicUsize::new(0),
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

    /// What the walk recorded: every shard's record, in shard order. Each
    /// walked directory's ignore inputs are read here, together: a walk that
    /// found something is never stored and never asks for its record.
    pub(super) fn walked(&self) -> Walked {
        let mut shards = self
            .shards
            .iter()
            .map(|shard| std::mem::take(&mut *lock(shard)))
            .collect::<Vec<_>>();
        self.read_directory_inputs(&mut shards);
        let oversize = self.weight.oversize.load(Ordering::Relaxed);
        let mut seen = Seen::default();
        for shard in shards {
            if !oversize {
                seen.searched.extend(shard.searched);
                seen.entries.extend(shard.entries);
            }
            seen.rules.extend(shard.rules);
            seen.untracked |= shard.untracked;
        }
        let mut entries = seen.searched;
        entries.append(&mut seen.entries);
        Walked {
            markers: if self.ignore_rules {
                octocode_tools_shared::page_memo::repository_markers(&self.root)
            } else {
                Vec::new()
            },
            root: self.root.clone(),
            entries,
            rules: seen.rules.into_iter().collect(),
            untracked: seen.untracked,
            oversize,
            started: self.started,
        }
    }

    /// Read the ignore inputs of every recorded directory, on this call's
    /// share of the cores, and record them in the shard of their directory.
    fn read_directory_inputs(&self, shards: &mut [Seen]) {
        if self.weight.oversize.load(Ordering::Relaxed) {
            return;
        }
        let dirs = shards
            .iter()
            .flat_map(|shard| shard.dirs.iter().map(|&at| shard.entries[at].as_path()))
            .collect::<Vec<_>>();
        let threads = octocode_engine::cpu::host().share().min(dirs.len());
        let mut inputs = if threads <= 1 {
            dirs.iter()
                .map(|dir| self.directory_inputs(dir))
                .collect::<Vec<_>>()
        } else {
            std::thread::scope(|scope| {
                let readers = dirs
                    .chunks(dirs.len().div_ceil(threads))
                    .map(|chunk| {
                        scope.spawn(move || {
                            chunk
                                .iter()
                                .map(|dir| self.directory_inputs(dir))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect::<Vec<_>>();
                readers
                    .into_iter()
                    .flat_map(|reader| reader.join().unwrap_or_default())
                    .collect()
            })
        }
        .into_iter();
        drop(dirs);
        for shard in shards {
            for _ in 0..shard.dirs.len() {
                if let Some(read) = inputs.next() {
                    shard.observe(read, &self.weight);
                }
            }
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
    /// ([`octocode_tools_shared::page_memo::directory_ignore_inputs_with`]).
    fn directory_inputs(&self, dir: &Path) -> DirectoryInputs {
        #[cfg(test)]
        self.input_reads.fetch_add(1, Ordering::Relaxed);
        octocode_tools_shared::page_memo::directory_ignore_inputs_with(
            dir,
            &self.root,
            self.policy.global_ignore(),
            |ignore| self.policy.validate_read(ignore).is_ok(),
        )
    }

    /// Record `path` when the walk admitted it, as a file it goes on to
    /// search when `file`. A directory's ignore inputs are not read here
    /// ([`Self::walked`] reads them), so the walk makes no file-system call
    /// for a record.
    fn admitted(&self, path: &Path, is_dir: bool, file: bool, allowed: bool) -> bool {
        if allowed {
            let entry = path.to_path_buf();
            let mut seen = self.shard();
            if file {
                seen.record_searched(entry, &self.weight);
            } else if is_dir && self.ignore_rules && path != self.root {
                seen.record_dir(entry, &self.weight);
            } else {
                seen.record(entry, &self.weight);
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
