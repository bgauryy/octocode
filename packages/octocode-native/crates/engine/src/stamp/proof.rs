//! The one proof that what a kept result read has not changed.
use super::{SourceStamp, epoch_nanos};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

fn provable(stamp: SourceStamp) -> Option<SourceStamp> {
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
}

/// Stamps this thread read from the file system.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use]
pub fn stamp_stats() -> usize {
    STAMP_STATS.with(std::cell::Cell::get)
}

/// `path`'s [`cache_stamp`]; with `settled_before`, only when it was last
/// written at least [`SourceStamp::SETTLE`] before then.
fn stamp_of(path: &Path, link: Link, settled_before: Option<SystemTime>) -> Option<SourceStamp> {
    #[cfg(any(test, feature = "test-hooks"))]
    STAMP_STATS.with(|stats| stats.set(stats.get() + 1));
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
    fn holds(self, path: &Path) -> bool {
        match self {
            Self::Stamp(link, stamp) => stamp_of(path, link, None) == stamp,
            Self::Presence(was) => exists(path) == was,
        }
    }
}

/// The one "what was read has not changed" proof a kept result holds: paths
/// with their state when they were added, taken before the reads they prove.
/// Shared, so a page checks it without copying the paths.
#[derive(Clone, Debug, Default)]
pub struct Proof(Arc<Vec<(PathBuf, Seen)>>);

impl Proof {
    fn add(&mut self, path: PathBuf, seen: Seen) {
        Arc::make_mut(&mut self.0).push((path, seen));
    }

    /// Stamp what `path` points at (absent counts).
    pub fn follow(&mut self, path: PathBuf) {
        let stamp = stamp_of(&path, Link::Follow, None);
        self.add(path, Seen::Stamp(Link::Follow, stamp));
    }

    /// Stamp the link at `path` itself (absent counts).
    pub fn link(&mut self, path: PathBuf) {
        let stamp = stamp_of(&path, Link::Itself, None);
        self.add(path, Seen::Stamp(Link::Itself, stamp));
    }

    /// Record only whether `path` exists.
    pub fn presence(&mut self, path: PathBuf) {
        let present = exists(&path);
        self.add(path, Seen::Presence(present));
    }

    /// The stamps of `paths`, all of which exist; `None` when one is gone, or,
    /// with `settled_before`, was written within [`SourceStamp::SETTLE`] before
    /// it (a write could race the reads the proof is taken for).
    #[must_use]
    pub fn capture(
        paths: impl IntoIterator<Item = PathBuf>,
        link: Link,
        settled_before: Option<SystemTime>,
    ) -> Option<Self> {
        let paths = paths.into_iter().collect::<Vec<_>>();
        let gone = AtomicBool::new(false);
        let stamps = in_chunks(&paths, |chunk| {
            chunk
                .iter()
                .map(|path| {
                    if gone.load(Ordering::Relaxed) {
                        return None;
                    }
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
        Some(Self(Arc::new(
            paths
                .into_iter()
                .zip(stamps.into_iter().flatten())
                .map(|(path, stamp)| (path, Seen::Stamp(link, Some(stamp))))
                .collect(),
        )))
    }

    /// This proof with `taken` too: stamps a walk read itself (`link` says
    /// how), which need no stat of their own.
    #[must_use]
    pub fn with_taken(mut self, link: Link, taken: Vec<(PathBuf, SourceStamp)>) -> Self {
        Arc::make_mut(&mut self.0).extend(
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
        Arc::make_mut(&mut self.0).extend(
            seen.into_iter()
                .map(|(path, present)| (path, Seen::Presence(present))),
        );
        self
    }

    /// Nothing stamped was written within the settle window before `started`.
    #[must_use]
    pub fn settled_before(&self, started: SystemTime) -> bool {
        let Some(settled_ns) = started
            .checked_sub(SourceStamp::SETTLE)
            .and_then(epoch_nanos)
        else {
            return false;
        };
        self.0.iter().all(|(_, seen)| match seen {
            Seen::Stamp(_, Some(stamp)) => stamp.modified_ns.max(stamp.changed_ns) <= settled_ns,
            Seen::Stamp(_, None) | Seen::Presence(_) => true,
        })
    }

    /// Every path is still as it was added.
    #[must_use]
    pub fn holds(&self) -> bool {
        let changed = AtomicBool::new(false);
        in_chunks(&self.0[..], |chunk| {
            chunk.iter().all(|(path, seen)| {
                let same = !changed.load(Ordering::Relaxed) && seen.holds(path);
                if !same {
                    changed.store(true, Ordering::Relaxed);
                }
                same
            })
        })
        .into_iter()
        .all(|same| same)
    }

    /// About how many bytes this proof holds.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.0
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
