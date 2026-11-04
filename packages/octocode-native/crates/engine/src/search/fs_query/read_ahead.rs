//! Read-ahead of the directories a listing walk reads next. The walk is
//! sequential and name-ordered, and most of its time is spent waiting on
//! directory reads: each directory it enters queues its subdirectories, and a
//! few workers read the most recently queued first, the walk's own depth-first
//! order, so the walk finds them read.
//!
//! Only the listing (names and entry types, sorted) and the directory's
//! modification time are read ahead, exactly as the walk reads them itself;
//! the walk still judges every entry (ignore rules, exclusions, the policy
//! callback) in its own order before it enters a directory, so a listing
//! read ahead for a directory the walk then leaves out is dropped unused. A
//! directory no worker has begun when the walk reaches it is read by the walk,
//! so queuing more than the workers can keep up with never makes it wait.
//! Nothing read ahead reaches a result unless the walk enters it, and the
//! walk's order and counts are the same with or without read-ahead.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Instant, SystemTime};

use crate::types::fs_resume::Child;

/// Directories a walk reads itself before it starts reading ahead: a small
/// listing never pays for the workers.
pub(super) const START_AFTER_DIRECTORIES: u32 = 16;
/// Workers reading ahead.
const WORKERS: usize = 4;

/// One directory read as the walk reads it: its modification time just
/// before the listing, then its entries sorted by name with the errors of
/// the entries that could not be read, or the error that kept it unread.
pub(super) struct Listing {
    pub(super) read_at: Option<SystemTime>,
    /// Its freshness stamp, from the same `lstat`.
    pub(super) stamp: Option<crate::stamp::SourceStamp>,
    pub(super) entries: std::io::Result<(Vec<Child>, Vec<std::io::Error>)>,
}

/// Read `base` as the walk does: with no deadline the read is never given up.
pub(super) fn read_listing(base: &Path) -> Listing {
    read_listing_until(base, None).unwrap_or_else(|| Listing {
        read_at: None,
        stamp: None,
        entries: Ok((Vec::new(), Vec::new())),
    })
}

/// Entries a listing reads between two looks at its deadline: a clock read
/// costs about what reading one entry does.
const ENTRIES_PER_DEADLINE_CHECK: usize = 64;

/// [`read_listing`], given up once the walk's `deadline` passes during it
/// (`None`): a directory too large to list within the walk's time budget is
/// left for the window that resumes the walk, which lists it whole.
pub(super) fn read_listing_until(base: &Path, deadline: Option<Instant>) -> Option<Listing> {
    let late = || deadline.is_some_and(|at| crate::scan_budget::walk_now() >= at);
    // Taken before the listing: a change after it moves the time on.
    let metadata = fs::symlink_metadata(base).ok();
    let read_at = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok());
    let stamp = metadata.as_ref().and_then(crate::stamp::SourceStamp::of);
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
        for (index, entry) in read_dir.enumerate() {
            if index % ENTRIES_PER_DEADLINE_CHECK == ENTRIES_PER_DEADLINE_CHECK - 1 && late() {
                return None;
            }
            match entry {
                Ok(entry) => children.push(Child {
                    name: entry.file_name(),
                    file_type: entry.file_type().ok(),
                }),
                Err(error) => errors.push(error),
            }
        }
        children.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Some((children, errors))
    });
    let entries = match entries {
        Ok(read) => Ok(read?),
        Err(error) => Err(error),
    };
    Some(Listing {
        read_at,
        stamp,
        entries,
    })
}

/// A directory the walk asked [`ReadAhead::take`] for.
pub(super) enum Taken {
    /// It was never queued: the walk reads it itself.
    NotQueued,
    Read(Listing),
    /// The walk's deadline passed while its read was under way: the listing
    /// is dropped when it arrives.
    Late,
}

#[cfg(test)]
static QUEUED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The directories under `root` a walk queued: all a worker can ever read,
/// whatever the scheduling.
#[cfg(test)]
pub(super) fn queued_under(root: &Path) -> Vec<PathBuf> {
    QUEUED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|dir| dir.starts_with(root))
        .cloned()
        .collect()
}

#[derive(Default)]
struct Read {
    /// Directories to read, the next one last.
    queue: Vec<PathBuf>,
    /// The walk is over: a worker takes nothing more.
    closed: bool,
    /// Listings read ahead, by directory.
    done: HashMap<PathBuf, Listing>,
    /// Directories a worker is reading now: the walk waits for these, since
    /// the read is under way.
    started: HashSet<PathBuf>,
    /// Directories the walk gave up on, unread or while their read was under
    /// way: a worker skips one it has not begun, and drops the listing of
    /// one it has.
    abandoned: HashSet<PathBuf>,
}

#[derive(Default)]
struct Shared {
    read: Mutex<Read>,
    /// A listing arrived, for the walk.
    arrived: Condvar,
    /// A directory was queued or the walk ended, for the workers.
    work: Condvar,
}

/// The workers of one walk and the directories queued to them. Dropping it
/// ends the workers once their current read is done.
pub(super) struct ReadAhead {
    shared: Arc<Shared>,
    /// Directories queued and not yet taken or dropped.
    pending: HashSet<PathBuf>,
}

impl ReadAhead {
    /// The workers, or `None` when none could start: the walk then reads
    /// every directory itself.
    pub(super) fn start() -> Option<Self> {
        let shared = Arc::new(Shared::default());
        let mut started = 0;
        for _ in 0..WORKERS {
            let shared = shared.clone();
            // A worker that cannot start leaves its jobs to the others.
            let worker = std::thread::Builder::new()
                .name("octocode-read-ahead".into())
                .spawn(move || work(&shared));
            started += usize::from(worker.is_ok());
        }
        (started > 0).then(|| Self {
            shared,
            pending: HashSet::new(),
        })
    }

    /// Queue `dirs` (in walk order) ahead of what is queued already, the
    /// first of them next.
    pub(super) fn queue(&mut self, dirs: impl Iterator<Item = PathBuf>) {
        let dirs = dirs
            .filter(|dir| self.pending.insert(dir.clone()))
            .collect::<Vec<_>>();
        if dirs.is_empty() {
            return;
        }
        #[cfg(test)]
        QUEUED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(dirs.iter().cloned());
        self.shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .queue
            .extend(dirs.into_iter().rev());
        self.shared.work.notify_all();
    }

    /// The listing of `dir` when a worker read it, waiting for a read under
    /// way until the walk's `deadline`. One no worker has begun is taken back:
    /// the walk reads it now instead of waiting behind the queue.
    pub(super) fn take(&mut self, dir: &Path, deadline: Option<Instant>) -> Taken {
        if !self.pending.remove(dir) {
            return Taken::NotQueued;
        }
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(listing) = read.done.remove(dir) {
                return Taken::Read(listing);
            }
            if !read.started.contains(dir) {
                read.abandoned.insert(dir.to_path_buf());
                return Taken::NotQueued;
            }
            let Some(at) = deadline else {
                read = self
                    .shared
                    .arrived
                    .wait(read)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            };
            let left = at.saturating_duration_since(crate::scan_budget::walk_now());
            if left.is_zero() {
                read.abandoned.insert(dir.to_path_buf());
                return Taken::Late;
            }
            read = self
                .shared
                .arrived
                .wait_timeout(read, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Wait until a worker has read `dir`, so a test's `take` finds it read.
    #[cfg(test)]
    pub(super) fn wait_until_read(&self, dir: &Path) {
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while !read.done.contains_key(dir) {
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

impl Drop for ReadAhead {
    fn drop(&mut self) {
        self.shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .closed = true;
        self.shared.work.notify_all();
    }
}

/// One worker: read the next queued directory the walk has not given up on,
/// until the walk ends.
fn work(shared: &Shared) {
    loop {
        let dir = {
            let mut read = shared.read.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if read.closed {
                    return;
                }
                match read.queue.pop() {
                    Some(dir) if read.abandoned.remove(&dir) => {}
                    Some(dir) => {
                        read.started.insert(dir.clone());
                        break dir;
                    }
                    None => {
                        read = shared
                            .work
                            .wait(read)
                            .unwrap_or_else(PoisonError::into_inner);
                    }
                }
            }
        };
        let listing = read_listing(&dir);
        let mut read = shared.read.lock().unwrap_or_else(PoisonError::into_inner);
        read.started.remove(&dir);
        if !read.abandoned.remove(&dir) {
            read.done.insert(dir, listing);
        }
        drop(read);
        shared.arrived.notify_all();
    }
}
