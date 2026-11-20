//! The one proof that what a kept result read has not changed.
use super::observed::{Held, Shown};
use super::{SourceStamp, epoch_nanos};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::SystemTime;

/// Metadata identity for cache reuse. On Unix the change time and inode are
/// part of it, and a missing change time cannot prove that a write
/// preserving size and mtime left the source unchanged. Elsewhere the
/// platform has no change time: size and modification time are the proof,
/// so memos keep working there instead of never storing.
#[must_use]
pub fn cache_stamp(meta: &std::fs::Metadata) -> Option<SourceStamp> {
    provable(SourceStamp::of(meta)?)
}

pub(super) fn provable(stamp: SourceStamp) -> Option<SourceStamp> {
    (cfg!(not(unix)) || stamp.changed_ns != 0).then_some(stamp)
}

/// How a path is stat'ed: through a symlink, or the link itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Link {
    Follow,
    Itself,
}

#[cfg(any(test, feature = "test-hooks"))]
thread_local! {
    static STAMP_STATS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PATH_STATS: std::cell::RefCell<std::collections::BTreeMap<PathBuf, usize>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Stamps this thread read from the file system.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use]
pub fn stamp_stats() -> usize {
    STAMP_STATS.with(std::cell::Cell::get)
}

/// Stamps this thread read of `path` from the file system.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use]
pub fn stamp_stats_of(path: &Path) -> usize {
    PATH_STATS.with(|stats| stats.borrow().get(path).copied().unwrap_or(0))
}

/// `path`'s [`cache_stamp`]; with `settled_before`, only when it was last
/// written at least [`SourceStamp::settle_for`] before then.
fn stamp_of(path: &Path, link: Link, settled_before: Option<SystemTime>) -> Option<SourceStamp> {
    #[cfg(any(test, feature = "test-hooks"))]
    {
        STAMP_STATS.with(|stats| stats.set(stats.get() + 1));
        PATH_STATS.with(|stats| *stats.borrow_mut().entry(path.to_path_buf()).or_default() += 1);
    }
    let meta = match link {
        Link::Follow => std::fs::metadata(path),
        Link::Itself => std::fs::symlink_metadata(path),
    }
    .ok()?;
    provable(match settled_before {
        None => SourceStamp::of(&meta)?,
        Some(started) => SourceStamp::settled(&meta, started)?,
    })
}

/// Whether `path` exists (a dangling link counts: its name is taken).
#[must_use]
pub fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// One path as a [`Proof`] saw it.
#[derive(Clone, Copy, Debug)]
enum Seen {
    /// Its stamp, read as [`Link`] says; `None` when it was absent.
    Stamp(Link, Option<SourceStamp>),
    /// Only whether anything has the name (a `.git` changes with every commit).
    Presence(bool),
}

impl Seen {
    /// Whether `path` is still as seen: shown so by a proof that held in
    /// this request ([`super::observing`]), else read; a stamp read is
    /// counted in `read`.
    fn holds(self, path: &Path, held: &Held, read: &AtomicUsize) -> bool {
        if !held.is_empty() {
            let shown = match self {
                Self::Stamp(link, Some(stamp)) => held.proves(path, link, stamp),
                Self::Stamp(link, None) => held.absent(path, link),
                Self::Presence(was) => held.presence(path) == Some(was),
            };
            if shown {
                return true;
            }
        }
        match self {
            Self::Stamp(link, stamp) => {
                read.fetch_add(1, Ordering::Relaxed);
                stamp_of(path, link, None) == stamp
            }
            Self::Presence(was) => exists(path) == was,
        }
    }

    fn shown(self) -> Shown {
        match self {
            Self::Stamp(link, stamp) => Shown::Stamp(link, stamp),
            Self::Presence(present) => Shown::Presence(present),
        }
    }
}

/// The one "what was read has not changed" proof a kept result holds: paths
/// with their state when they were added, taken before the reads they prove.
/// Shared, so a page checks it without copying the paths.
#[derive(Clone, Debug, Default)]
pub struct Proof {
    seen: Arc<Vec<(PathBuf, Seen)>>,
    /// Stamps read from the file system to build it.
    stamps_read: usize,
    /// `seen` ordered by path bytes, built on the first lookup
    /// ([`Self::shown`]) and shared by the clones of an unchanged proof.
    index: Arc<std::sync::OnceLock<Box<[u32]>>>,
}

/// What [`Proof::check`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checked {
    /// Every path is still as it was added.
    pub holds: bool,
    /// Stamps the check read from the file system: it stops at the first
    /// change it finds.
    pub stamps_read: usize,
}

impl Proof {
    fn add(&mut self, path: PathBuf, seen: Seen) {
        self.changed().push((path, seen));
    }

    /// `seen` to change: the lookup index no longer stands for it.
    fn changed(&mut self) -> &mut Vec<(PathBuf, Seen)> {
        self.index = Arc::default();
        Arc::make_mut(&mut self.seen)
    }

    /// Whether `other` is this proof, or a clone of it.
    pub(super) fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.seen, &other.seen)
    }

    /// What this proof saw of `path`, found by its index.
    pub(super) fn shown(&self, path: &Path) -> Option<Shown> {
        let bytes = |index: u32| {
            usize::try_from(index)
                .ok()
                .and_then(|at| self.seen.get(at))
                .map_or(&[][..], |(path, _)| path.as_os_str().as_encoded_bytes())
        };
        let index = self.index.get_or_init(|| {
            let mut order =
                (0..u32::try_from(self.seen.len()).unwrap_or(u32::MAX)).collect::<Vec<_>>();
            order.sort_by(|left, right| bytes(*left).cmp(bytes(*right)));
            order.into_boxed_slice()
        });
        let wanted = path.as_os_str().as_encoded_bytes();
        let at = index
            .binary_search_by(|probe| bytes(*probe).cmp(wanted))
            .ok()?;
        let seen = usize::try_from(index[at]).ok()?;
        self.seen.get(seen).map(|(_, seen)| seen.shown())
    }

    /// Stamp what `path` points at (absent counts).
    pub fn follow(&mut self, path: PathBuf) {
        let stamp = stamp_of(&path, Link::Follow, None);
        self.stamps_read += 1;
        self.add(path, Seen::Stamp(Link::Follow, stamp));
    }

    /// Stamp the link at `path` itself (absent counts).
    pub fn link(&mut self, path: PathBuf) {
        let stamp = stamp_of(&path, Link::Itself, None);
        self.stamps_read += 1;
        self.add(path, Seen::Stamp(Link::Itself, stamp));
    }

    /// Stamps read from the file system to build this proof: none for
    /// stamps a walk took itself ([`Self::with_taken`]) or presences.
    #[must_use]
    pub fn stamps_read(&self) -> usize {
        self.stamps_read
    }

    /// Record only whether `path` exists.
    pub fn presence(&mut self, path: PathBuf) {
        let present = exists(&path);
        self.add(path, Seen::Presence(present));
    }

    /// The stamps of `paths`, all of which exist; `None` when one is gone, or,
    /// with `settled_before`, was written within [`SourceStamp::settle_for`] before
    /// it (a write could race the reads the proof is taken for).
    #[must_use]
    pub fn capture(
        paths: impl IntoIterator<Item = PathBuf>,
        link: Link,
        settled_before: Option<SystemTime>,
    ) -> Option<Self> {
        let paths = paths.into_iter().collect::<Vec<_>>();
        let gone = AtomicBool::new(false);
        let read = AtomicUsize::new(0);
        let stamps = in_chunks(&paths, |chunk| {
            chunk
                .iter()
                .map(|path| {
                    if gone.load(Ordering::Relaxed) {
                        return None;
                    }
                    read.fetch_add(1, Ordering::Relaxed);
                    let stamp = stamp_of(path, link, settled_before);
                    if stamp.is_none() {
                        gone.store(true, Ordering::Relaxed);
                    }
                    stamp
                })
                .collect::<Option<Vec<_>>>()
        })
        .into_iter()
        .collect::<Option<Vec<_>>>()?;
        Some(Self {
            seen: Arc::new(
                paths
                    .into_iter()
                    .zip(stamps.into_iter().flatten())
                    .map(|(path, stamp)| (path, Seen::Stamp(link, Some(stamp))))
                    .collect(),
            ),
            stamps_read: read.into_inner(),
            index: Arc::default(),
        })
    }

    /// [`Self::capture`] of `paths`, joined by `taken`: stamps a walk read
    /// itself as `link` reads ([`cache_stamp`]), which cost no stat of their
    /// own. They meet the capture's rules: `None` when one proves nothing,
    /// or, with `settled_before`, was written within its
    /// [`SourceStamp::settle_for`] window before it.
    #[must_use]
    pub fn capture_with_taken(
        paths: impl IntoIterator<Item = PathBuf>,
        taken: Vec<(PathBuf, SourceStamp)>,
        link: Link,
        settled_before: Option<SystemTime>,
    ) -> Option<Self> {
        let admitted = |stamp: &SourceStamp| {
            provable(*stamp).is_some()
                && settled_before.is_none_or(|started| {
                    epoch_nanos(started).is_some_and(|now| stamp.settled_by(now))
                })
        };
        if !taken.iter().all(|(_, stamp)| admitted(stamp)) {
            return None;
        }
        Some(Self::capture(paths, link, settled_before)?.with_taken(link, taken))
    }

    /// This proof with `taken` too: stamps a walk read itself (`link` says
    /// how), which need no stat of their own.
    #[must_use]
    pub fn with_taken(mut self, link: Link, taken: Vec<(PathBuf, SourceStamp)>) -> Self {
        self.changed().extend(
            taken
                .into_iter()
                .filter(|(_, stamp)| provable(*stamp).is_some())
                .map(|(path, stamp)| (path, Seen::Stamp(link, Some(stamp)))),
        );
        self
    }

    /// This proof with each path's presence, as already observed.
    #[must_use]
    pub fn with_presence(mut self, seen: Vec<(PathBuf, bool)>) -> Self {
        self.changed().extend(
            seen.into_iter()
                .map(|(path, present)| (path, Seen::Presence(present))),
        );
        self
    }

    /// This proof with everything `other` proves too (and the stamps read
    /// to build it).
    #[must_use]
    pub fn with(mut self, other: &Self) -> Self {
        self.changed().extend(other.seen.iter().cloned());
        self.stamps_read += other.stamps_read;
        self
    }

    /// Nothing stamped was written within the settle window before `started`.
    #[must_use]
    pub fn settled_before(&self, started: SystemTime) -> bool {
        let Some(started_ns) = epoch_nanos(started) else {
            return false;
        };
        self.seen.iter().all(|(_, seen)| match seen {
            Seen::Stamp(_, Some(stamp)) => stamp.settled_by(started_ns),
            Seen::Stamp(_, None) | Seen::Presence(_) => true,
        })
    }

    /// Every path is still as it was added.
    #[must_use]
    pub fn holds(&self) -> bool {
        self.check().holds
    }

    /// Whether every path is still as it was added, and the stamps that
    /// took reading.
    #[must_use]
    ///
    /// Inside [`super::observing`], a path a proof that already held in the
    /// request shows unchanged costs no stat, and this proof, once it holds,
    /// joins those proofs ([`super::Held`]).
    pub fn check(&self) -> Checked {
        let changed = AtomicBool::new(false);
        let read = AtomicUsize::new(0);
        let observed = super::observation();
        let held = super::held();
        let holds = in_chunks(&self.seen[..], |chunk| {
            chunk.iter().all(|(path, seen)| {
                let same = !changed.load(Ordering::Relaxed) && seen.holds(path, &held, &read);
                if !same {
                    changed.store(true, Ordering::Relaxed);
                }
                same
            })
        })
        .into_iter()
        .all(|same| same);
        let stamps_read = read.into_inner();
        if let Some(observed) = observed {
            observed.count_stats(stamps_read);
            if holds {
                observed.hold(self);
            }
        }
        Checked { holds, stamps_read }
    }

    /// About how many bytes this proof holds.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.seen
            .iter()
            .map(|(path, _)| path.as_os_str().len() + std::mem::size_of::<(PathBuf, Seen)>())
            .sum()
    }
}

/// Stats one stamp thread takes at least: a scoped thread costs tens of
/// microseconds, about what a few hundred warm stats take, so a smaller set
/// is stamped on the calling thread.
const STATS_PER_WORKER: usize = 256;
/// Most threads one proof is stamped on: the file-open fan-out the line
/// counts measured best (`structure_search::rows::line_counts`).
const STAT_WORKERS: usize = 4;

/// `job` of each contiguous chunk of `items`, in order: one chunk per
/// [`STATS_PER_WORKER`] items, on at most [`STAT_WORKERS`] scoped threads
/// (one chunk on the calling thread when that is all there is).
fn in_chunks<T: Sync, R: Send>(items: &[T], job: impl Fn(&[T]) -> R + Sync) -> Vec<R> {
    let workers = crate::cpu::host()
        .share()
        .min(STAT_WORKERS)
        .min(items.len() / STATS_PER_WORKER)
        .max(1);
    if workers < 2 {
        return vec![job(items)];
    }
    std::thread::scope(|scope| {
        let job = &job;
        let running = items
            .chunks(items.len().div_ceil(workers))
            .map(|chunk| scope.spawn(move || job(chunk)))
            .collect::<Vec<_>>();
        running
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests;
