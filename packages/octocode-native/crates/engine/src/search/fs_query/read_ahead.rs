//! Bounded read-ahead of the directories a listing walk reads next. The walk
//! is sequential and name-ordered, and on a tree larger than the kernel's
//! directory cache most of its time is spent waiting on directory reads:
//! while it walks one directory's subtree, a few workers read the listings
//! of the sibling directories after it, so the walk finds them read.
//!
//! Only the listing (names and entry types, sorted) and the directory's
//! modification time are read ahead, exactly as the walk reads them itself;
//! the walk still judges every entry (ignore rules, exclusions, the policy
//! callback) in its own order before it enters a directory, so a listing
//! read ahead for a directory the walk then leaves out is dropped unused.
//! Nothing read ahead reaches a result unless the walk enters it, and the
//! walk's order and counts are the same with or without read-ahead.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError, mpsc};
use std::time::SystemTime;

use crate::search::fs_resume::Child;

/// Directories a walk reads itself before it starts reading ahead: a small
/// listing never pays for the workers.
pub(super) const START_AFTER_DIRECTORIES: u32 = 16;
/// Workers reading ahead.
const WORKERS: usize = 4;
/// Sibling directories after the one the walk enters that are read ahead.
const SIBLINGS_AHEAD: usize = 8;

/// One directory read as the walk reads it: its modification time just
/// before the listing, then its entries sorted by name with the errors of
/// the entries that could not be read, or the error that kept it unread.
pub(super) struct Listing {
    pub(super) read_at: Option<SystemTime>,
    pub(super) entries: std::io::Result<(Vec<Child>, Vec<std::io::Error>)>,
}

/// Read `base` as the walk does.
pub(super) fn read_listing(base: &Path) -> Listing {
    // Taken before the listing: a change after it moves the time on.
    let read_at = fs::symlink_metadata(base)
        .and_then(|metadata| metadata.modified())
        .ok();
    let entries = fs::read_dir(base).map(|read_dir| {
        // `fs::read_dir` yields entries in raw OS order (hash order on APFS,
        // etc.), so without sorting the result set — and its silent
        // truncation at the limit — would vary run-to-run. Sort by file name
        // for deterministic, stable output (names are unique in a directory,
        // and each is taken once: a comparison that asked the entry for its
        // name would allocate it every time). The entry type comes with the
        // directory listing on common platforms.
        let mut children = Vec::new();
        let mut errors = Vec::new();
        for entry in read_dir {
            match entry {
                Ok(entry) => children.push(Child {
                    name: entry.file_name(),
                    file_type: entry.file_type().ok(),
                }),
                Err(error) => errors.push(error),
            }
        }
        children.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        (children, errors)
    });
    Listing { read_at, entries }
}

#[derive(Default)]
struct Read {
    /// Listings read ahead, by directory.
    done: HashMap<PathBuf, Listing>,
    /// Directories the walk left out while their read was under way: the
    /// listing is dropped when it arrives.
    abandoned: HashSet<PathBuf>,
}

#[derive(Default)]
struct Shared {
    read: Mutex<Read>,
    arrived: Condvar,
}

/// The workers of one walk and the directories queued to them. Dropping it
/// ends the workers once their current read is done.
pub(super) struct ReadAhead {
    shared: Arc<Shared>,
    jobs: mpsc::Sender<PathBuf>,
    /// Directories queued and not yet taken or dropped.
    pending: HashSet<PathBuf>,
}

impl ReadAhead {
    /// The workers, or `None` when none could start: the walk then reads
    /// every directory itself.
    pub(super) fn start() -> Option<Self> {
        let shared = Arc::new(Shared::default());
        let (jobs, queue) = mpsc::channel::<PathBuf>();
        let queue = Arc::new(Mutex::new(queue));
        let mut started = 0;
        for _ in 0..WORKERS {
            let (queue, shared) = (queue.clone(), shared.clone());
            // A worker that cannot start leaves its jobs to the others.
            let worker = std::thread::Builder::new()
                .name("octocode-read-ahead".into())
                .spawn(move || {
                    loop {
                        let job = queue.lock().unwrap_or_else(PoisonError::into_inner).recv();
                        let Ok(dir) = job else { return };
                        let listing = read_listing(&dir);
                        let mut read = shared.read.lock().unwrap_or_else(PoisonError::into_inner);
                        if !read.abandoned.remove(&dir) {
                            read.done.insert(dir, listing);
                        }
                        drop(read);
                        shared.arrived.notify_all();
                    }
                });
            started += usize::from(worker.is_ok());
        }
        (started > 0).then(|| Self {
            shared,
            jobs,
            pending: HashSet::new(),
        })
    }

    /// Queue the first [`SIBLINGS_AHEAD`] of `dirs` (in walk order) not
    /// queued already.
    pub(super) fn queue(&mut self, dirs: impl Iterator<Item = PathBuf>) {
        for dir in dirs.take(SIBLINGS_AHEAD) {
            if !self.pending.contains(&dir) && self.jobs.send(dir.clone()).is_ok() {
                self.pending.insert(dir);
            }
        }
    }

    /// The listing of `dir` when it was queued, waiting for its read.
    pub(super) fn take(&mut self, dir: &Path) -> Option<Listing> {
        if !self.pending.remove(dir) {
            return None;
        }
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(listing) = read.done.remove(dir) {
                return Some(listing);
            }
            read = self
                .shared
                .arrived
                .wait(read)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Drop the listing of `dir` when it was queued and not taken: the walk
    /// left it out.
    pub(super) fn drop_unused(&mut self, dir: &Path) {
        if !self.pending.remove(dir) {
            return;
        }
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if read.done.remove(dir).is_none() {
            read.abandoned.insert(dir.to_path_buf());
        }
    }
}
