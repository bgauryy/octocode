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
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Instant, SystemTime};

use crate::types::fs_resume::Child;

/// Directories a walk reads itself before it starts reading ahead: a small
/// listing never pays for the workers.
pub(crate) const START_AFTER_DIRECTORIES: u32 = 16;
/// Listings each worker may hold ahead of the walk, read or under way: the
/// walk takes them in order, so more would only hold memory.
const AHEAD_PER_WORKER: usize = 2;

/// What a listing reads of its directory besides the names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Detail {
    /// The directory's own modification time and stamp, from an `lstat`
    /// taken before the listing: what a pin or a resume proves a walk by.
    Stamped,
    /// The names and entry types only: a walk that pins the files it reads
    /// needs no stamp of the directories (one `lstat` fewer per directory).
    Names,
}

/// One directory read as the walk reads it: its modification time just
/// before the listing, then its entries sorted by name with the errors of
/// the entries that could not be read, or the error that kept it unread.
pub(crate) struct Listing {
    pub(crate) read_at: Option<SystemTime>,
    /// Its freshness stamp, from the same `lstat`.
    pub(crate) stamp: Option<crate::stamp::SourceStamp>,
    pub(crate) entries: std::io::Result<(Vec<Child>, Vec<std::io::Error>)>,
}

/// Read `base` as the walk does: with no deadline the read is never given up.
#[cfg(test)]
pub(crate) fn read_listing(base: &Path) -> Listing {
    read_listing_with(base, Detail::Stamped)
}

/// [`read_listing`] of the `detail` a caller needs.
pub(crate) fn read_listing_with(base: &Path, detail: Detail) -> Listing {
    read_listing_until(base, None, detail).unwrap_or_else(|| Listing {
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
pub(crate) fn read_listing_until(
    base: &Path,
    deadline: Option<Instant>,
    detail: Detail,
) -> Option<Listing> {
    let late = || deadline.is_some_and(|at| crate::scan_budget::walk_now() >= at);
    #[cfg(test)]
    LISTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(base.to_path_buf());
    // Taken before the listing: a change after it moves the time on.
    let metadata = (detail == Detail::Stamped)
        .then(|| fs::symlink_metadata(base).ok())
        .flatten();
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
                Ok(entry) => match entry.file_type() {
                    Ok(file_type) => children.push(Child {
                        name: entry.file_name(),
                        file_type: Some(file_type),
                    }),
                    // A names-only listing is the entry a walk would
                    // report, so an entry whose type cannot be read is
                    // an error naming it, not an entry of unknown type.
                    Err(error) if detail == Detail::Names => errors.push(std::io::Error::new(
                        error.kind(),
                        super::ordered::path_error(&entry.path(), &error),
                    )),
                    Err(_) => children.push(Child {
                        name: entry.file_name(),
                        file_type: None,
                    }),
                },
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
pub(crate) enum Taken {
    /// It was never queued: the walk reads it itself.
    NotQueued,
    Read(Listing),
    /// The walk's deadline passed while its read was under way: the listing
    /// is dropped when it arrives.
    Late,
}

#[cfg(test)]
static QUEUED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

#[cfg(test)]
static LISTED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The directories under `root` any walk listed, itself or read ahead
/// (tests): one no walk may list is left out of the rules.
#[cfg(test)]
pub(crate) fn listed_under(root: &Path) -> Vec<PathBuf> {
    LISTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|dir| dir.starts_with(root))
        .cloned()
        .collect()
}

/// The directories under `root` a walk queued: all a worker can ever read,
/// whatever the scheduling.
#[cfg(test)]
pub(crate) fn queued_under(root: &Path) -> Vec<PathBuf> {
    QUEUED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|dir| dir.starts_with(root))
        .cloned()
        .collect()
}

/// A directory queued to the workers: one allocation shared by every
/// structure that names it.
pub(crate) type Key = Arc<Path>;

#[derive(Default)]
struct Read {
    /// Directories to read, the next one last. At most `limit` are queued:
    /// when more arrive, the ones furthest from the walk are given back to
    /// it, which reads them itself.
    queue: Vec<Key>,
    /// The walk is over: a worker takes nothing more.
    closed: bool,
    /// Directories queued, being read or read, and not yet taken or dropped.
    pending: HashSet<Key>,
    /// Listings read ahead, by directory.
    done: HashMap<Key, Listing>,
    /// Directories a worker is reading now: the walk waits for these, since
    /// the read is under way.
    started: HashSet<Key>,
    /// Directories the walk gave up on while their read was under way: the
    /// worker drops their listing.
    abandoned: HashSet<Key>,
}

struct Shared {
    read: Mutex<Read>,
    /// Listings read and held (done or under way) at most, so a flat
    /// directory of many subdirectories is never read whole into memory.
    limit: usize,
    /// What each listing reads besides the names.
    detail: Detail,
    /// The most listings ever held (tests).
    #[cfg(test)]
    peak: std::sync::atomic::AtomicUsize,
    /// A listing arrived, for the walk.
    arrived: Condvar,
    /// A directory was queued or the walk ended, for the workers.
    work: Condvar,
}

impl Shared {
    /// Record the listings held now toward the peak (tests).
    #[cfg(test)]
    fn note_held(&self, read: &Read) {
        self.peak.fetch_max(
            read.done.len() + read.started.len(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// The workers of one walk and the directories queued to them. Dropping it
/// ends the workers once their current read is done.
pub(crate) struct ReadAhead {
    shared: Arc<Shared>,
}

impl ReadAhead {
    /// `workers` threads (at least one) reading what each listing needs
    /// (`detail`), or `None` when none could start: the walk then reads
    /// every directory itself.
    pub(crate) fn start(workers: usize, detail: Detail) -> Option<Self> {
        let workers = workers.max(1);
        let shared = Arc::new(Shared {
            read: Mutex::default(),
            limit: AHEAD_PER_WORKER * workers,
            detail,
            #[cfg(test)]
            peak: std::sync::atomic::AtomicUsize::new(0),
            arrived: Condvar::default(),
            work: Condvar::default(),
        });
        let mut started = 0;
        for _ in 0..workers {
            let shared = shared.clone();
            // A worker that cannot start leaves its jobs to the others.
            let worker = std::thread::Builder::new()
                .name("octocode-read-ahead".into())
                .spawn(move || work(&shared));
            started += usize::from(worker.is_ok());
        }
        (started > 0).then(|| Self { shared })
    }

    /// Listings held at most (read or under way), so the nearest that many
    /// directories are worth offering.
    pub(crate) fn window(&self) -> usize {
        self.shared.limit
    }

    /// Queue `dirs` (in walk order) ahead of what is queued already, the
    /// first of them next. Past the bound the ones furthest from the walk
    /// are not queued: it reads those itself when it reaches them.
    pub(crate) fn queue(&self, dirs: impl Iterator<Item = Key>) {
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let dirs = dirs
            .filter(|dir| read.pending.insert(Key::clone(dir)))
            .collect::<Vec<_>>();
        if dirs.is_empty() {
            return;
        }
        #[cfg(test)]
        QUEUED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(dirs.iter().map(|dir| dir.to_path_buf()));
        let dirs_queued = dirs.len();
        read.queue.extend(dirs.into_iter().rev());
        let excess = read.queue.len().saturating_sub(self.shared.limit);
        let given_back = read.queue.drain(..excess).collect::<Vec<_>>();
        for dir in given_back {
            read.pending.remove(&dir);
        }
        let queued = read.queue.len();
        drop(read);
        // A worker for each directory queued, no more woken than that.
        for _ in 0..queued.min(dirs_queued) {
            self.shared.work.notify_one();
        }
    }

    /// The listing of `dir` when a worker read it, waiting for a read under
    /// way until the walk's `deadline`. One no worker has begun is taken back:
    /// the walk reads it now instead of waiting behind the queue.
    pub(crate) fn take(&self, dir: &Path, deadline: Option<Instant>) -> Taken {
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(key) = read.pending.get(dir).cloned() else {
            return Taken::NotQueued;
        };
        loop {
            if let Some(listing) = read.done.remove(dir) {
                read.pending.remove(dir);
                drop(read);
                // One listing fewer is held: one worker may read the next.
                self.shared.work.notify_one();
                return Taken::Read(listing);
            }
            if !read.started.contains(dir) {
                read.queue.retain(|queued| **queued != *dir);
                read.pending.remove(dir);
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
                read.pending.remove(dir);
                read.abandoned.insert(key);
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

    /// The most listings held at once, and the most allowed (tests).
    #[cfg(test)]
    pub(crate) fn held_peak_and_limit(&self) -> (usize, usize) {
        (
            self.shared.peak.load(std::sync::atomic::Ordering::Relaxed),
            self.shared.limit,
        )
    }

    /// Directories queued and not yet begun (tests).
    #[cfg(test)]
    pub(crate) fn queued_len(&self) -> usize {
        self.shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .queue
            .len()
    }

    /// Wait until a worker has read `dir`, so a test's `take` finds it read.
    #[cfg(test)]
    pub(crate) fn wait_until_read(&self, dir: &Path) {
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
    pub(crate) fn drop_unused(&self, dir: &Path) {
        let mut read = self
            .shared
            .read
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(key) = read.pending.take(dir) else {
            return;
        };
        if read.done.remove(dir).is_some() {
            drop(read);
            self.shared.work.notify_one();
        } else if read.started.contains(dir) {
            read.abandoned.insert(key);
        } else {
            read.queue.retain(|queued| **queued != *dir);
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
                // Past the bound a worker waits for the walk to take a
                // listing; the walk reads a directory no worker has begun
                // itself, so it never waits on a read held back here.
                if read.done.len() + read.started.len() >= shared.limit {
                    read = shared
                        .work
                        .wait(read)
                        .unwrap_or_else(PoisonError::into_inner);
                    continue;
                }
                match read.queue.pop() {
                    Some(dir) => {
                        read.started.insert(dir.clone());
                        #[cfg(test)]
                        shared.note_held(&read);
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
        let listing = read_listing_with(&dir, shared.detail);
        let mut read = shared.read.lock().unwrap_or_else(PoisonError::into_inner);
        read.started.remove(&dir);
        if !read.abandoned.remove(&dir) {
            read.done.insert(dir, listing);
        }
        #[cfg(test)]
        shared.note_held(&read);
        drop(read);
        shared.arrived.notify_all();
    }
}
