//! One request's proven stamps ([`observing`]): a path one proof of the
//! request already stamped costs no second stat when another proof, or a
//! memo lookup, compares it with the same stamp.
//!
//! Inside [`observing`], every [`super::Proof`] whose check holds joins the
//! request's observation: each path it holds is, as of that check, exactly as
//! it was added. A later comparison of such a path with an expected stamp is
//! answered from it ([`Held::proves`]) instead of the file system. A stamp of
//! the path itself (`lstat`) answers a comparison through links too: an equal
//! stamp names the same inode, which a link never is, so the path was no link
//! and both reads agree. A stamp read through links answers only its own
//! kind. A comparison that finds nothing, or a different stamp, reads the file
//! system, so nothing is ever judged unchanged that a held proof does not
//! show unchanged.
//!
//! What is served is the request's first look at a path: a check served from
//! it judges the path as the request saw it then. A stamp taken after a read,
//! to show the file did not change while it was read, must be fresh: those
//! never ask here. Outside [`observing`] nothing changes: every check reads.
use super::SourceStamp;
use super::proof::{Link, Proof};
use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The proofs that held in one request ([`observing`]), each once.
#[derive(Debug, Default)]
pub struct Observed {
    /// Replaced, never changed in place, so a snapshot ([`held`]) is one
    /// `Arc` clone however often a request asks.
    held: Mutex<Arc<Vec<Proof>>>,
    /// Stats its proof checks made of the file system.
    stats: AtomicUsize,
}

impl Observed {
    /// Stats its proof checks made of the file system.
    #[must_use]
    pub fn stats(&self) -> usize {
        self.stats.load(Ordering::Relaxed)
    }

    /// The proofs that held so far.
    #[must_use]
    pub fn held_proofs(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Arc<Vec<Proof>>> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Join `proof`, unless the same proof (a clone of it) already held: a
    /// kept proof checked on every lookup of a request is held once.
    pub(super) fn hold(&self, proof: &Proof) {
        let mut held = self.lock();
        if !held.iter().any(|known| known.is_same(proof)) {
            Arc::make_mut(&mut held).push(proof.clone());
        }
    }

    pub(super) fn count_stats(&self, stats: usize) {
        self.stats.fetch_add(stats, Ordering::Relaxed);
    }

    fn held(&self) -> Held {
        Held(Arc::clone(&self.lock()))
    }
}

/// What the proofs that held in this request show, taken once for many
/// comparisons ([`held`]).
#[derive(Clone, Debug, Default)]
pub struct Held(Arc<Vec<Proof>>);

/// One held path, as a held proof shows it (`Proof::shown`).
pub(super) enum Shown {
    /// Its stamp as `link` read it; `None`: absent.
    Stamp(Link, Option<SourceStamp>),
    /// Only whether it existed.
    Presence(bool),
}

impl Held {
    /// Whether a held proof shows `path`, read as `link` says, at exactly
    /// `expected` (see the module docs for a stamp of the path itself).
    #[must_use]
    pub fn proves(&self, path: &Path, link: Link, expected: SourceStamp) -> bool {
        self.0
            .iter()
            .filter_map(|proof| proof.shown(path))
            .any(|shown| match shown {
                Shown::Stamp(seen, Some(stamp)) => {
                    stamp == expected
                        && serves(seen, link)
                        && super::proof::provable(stamp).is_some()
                }
                Shown::Stamp(_, None) | Shown::Presence(_) => false,
            })
    }

    /// Whether a held proof shows `path` absent as `link` reads it.
    pub(super) fn absent(&self, path: &Path, link: Link) -> bool {
        self.0
            .iter()
            .filter_map(|proof| proof.shown(path))
            // Nothing has the name: absent both ways.
            .any(|shown| {
                matches!(shown, Shown::Stamp(seen, None) if seen == link || seen == Link::Itself)
            })
    }

    /// Whether anything had `path`'s name, when a held proof says.
    pub(super) fn presence(&self, path: &Path) -> Option<bool> {
        self.0
            .iter()
            .filter_map(|proof| proof.shown(path))
            .find_map(|shown| match shown {
                Shown::Stamp(_, Some(_)) => Some(true),
                Shown::Stamp(Link::Itself, None) => Some(false),
                Shown::Presence(present) => Some(present),
                Shown::Stamp(Link::Follow, None) => None,
            })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Whether a stamp read as `seen` answers a comparison read as `wanted`: the
/// path itself answers both (an equal stamp is the same inode, never a link;
/// only where stamps carry the inode), a read through links only its own.
fn serves(seen: Link, wanted: Link) -> bool {
    seen == wanted || (cfg!(unix) && seen == Link::Itself)
}

thread_local! {
    /// The observation work on this thread joins ([`observing`]).
    static CURRENT: RefCell<Option<Arc<Observed>>> = const { RefCell::new(None) };
}

/// Run `work` with `observed` as this thread's observation (`None`: none),
/// restoring the one before it afterwards. `work` must not wait on other work
/// of this thread (an async task's `await`): another request could run on the
/// thread meanwhile.
pub fn observing<T>(observed: Option<Arc<Observed>>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Observed>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            CURRENT.with(|current| *current.borrow_mut() = before);
        }
    }
    let _restore = Restore(CURRENT.with(|current| current.replace(observed)));
    work()
}

/// This thread's observation, to hand to work on another thread
/// ([`observing`]).
#[must_use]
pub fn observation() -> Option<Arc<Observed>> {
    CURRENT.with(|current| current.borrow().clone())
}

/// What the proofs that held so far in this thread's observation show
/// (empty outside one).
#[must_use]
pub fn held() -> Held {
    observation()
        .map(|observed| observed.held())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
