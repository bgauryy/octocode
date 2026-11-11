//! What a row-limit window of a listing keeps for the window after it, so a
//! listing read to its end costs one walk of its scope, not one per window.
//!
//! A listing that walks its whole scope keeps its ranked entries ([`Ranked`]);
//! a walk-order listing, which stops at its window, keeps where it stands
//! ([`Carried`]). Both are candidates under the rules of every stored result:
//! they serve only while their proofs hold, and a miss (evicted, expired,
//! another process, a proof that failed) walks instead.
//!
//! The next window's query pins what the windows before it listed, with the
//! digest of a [`ListingDigest`] in its `snapshot`: a walk that does not hash
//! to it restarts the listing, since its rows moved and a window cut by row
//! count would repeat or skip one. A whole-scope listing pins all its entries,
//! a walk-order one those it has listed: what it has not reached it has not
//! seen, and it lists what is there when it gets there. A walk cut by time pins
//! the rows through its cut the same way ([`super::resume`]).
use super::super::page_memo::{PageMemo, Proven};
use super::listing::Listing;
use super::memo;
use super::rows::{self, Lines};
use super::walk_policy::{LeftSoFar, WalkLeft};
use super::walked::{WalkMeta, Walked};
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use octocode_engine::stamp::{Link, Proof};
use octocode_engine::types::{FileSystemEntry, FileSystemQueryResult, WalkCursor};
use std::mem::size_of;
use std::path::Path;
use std::sync::Arc;

mod pins;

pub(super) use pins::{ListingDigest, PinnedWalk, Pins, pinned, pins, ranked_pins};

/// A whole-scope listing's entries in listing order, and what its windows read
/// beside their own rows.
struct Ranked {
    entries: Vec<FileSystemEntry>,
    /// Each entry's line count, when the walk counted them.
    lines: Vec<Option<Lines>>,
    /// What the walk saw beside its rows.
    meta: WalkMeta,
    /// The listing's directories and rule files.
    dirs: Proof,
}

impl Ranked {
    /// The heap this listing holds.
    fn weight(&self) -> usize {
        size_of::<Self>()
            + self.entries.capacity() * size_of::<FileSystemEntry>()
            + self
                .entries
                .iter()
                .map(|entry| {
                    entry.path.capacity() + entry.relative_path.capacity() + entry.name.capacity()
                })
                .sum::<usize>()
            + self.lines.capacity() * size_of::<Option<Lines>>()
            + self.meta.weight()
    }
}

static RANKED: PageMemo<Proven<Ranked>> = PageMemo::new();

/// The pin of the ranking each listing shape ([`shape_key`]) last kept, so a
/// repeat of its first window is served from [`RANKED`] while that ranking's
/// proof holds, instead of walking the scope again.
static FIRSTS: PageMemo<String> = PageMemo::new();

#[cfg(test)]
thread_local! {
    /// Whether this thread's first windows may be read from [`FIRSTS`].
    static SERVES_FIRSTS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// While the guard lives, this thread walks every first window, as a process
/// that kept nothing would: for a test of how the walk itself runs.
#[cfg(test)]
pub(super) struct WalkingFirsts(bool);

#[cfg(test)]
pub(super) fn walking_firsts() -> WalkingFirsts {
    WalkingFirsts(SERVES_FIRSTS.replace(false))
}

#[cfg(test)]
impl Drop for WalkingFirsts {
    fn drop(&mut self) {
        SERVES_FIRSTS.set(self.0);
    }
}

/// Whether a first window may be read from the listing its shape kept.
#[cfg(test)]
fn serves_firsts() -> bool {
    SERVES_FIRSTS.get()
}

/// Whether a first window may be read from the listing its shape kept.
#[cfg(not(test))]
const fn serves_firsts() -> bool {
    true
}

/// What a listing's rows depend on before any row: its walk, view, sort and
/// layout ([`ListingDigest::new`]), never its window or page.
pub(super) fn shape_key<Q: Listing>(q: &Q, root: &Path) -> String {
    ListingDigest::new(q, root).hex()
}

/// Keep a whole-scope listing whose first window is `walk`, under the digest
/// its next window pins. Its proof holds every directory and rule file it
/// read, and with a view that shows metadata every entry, since later windows
/// show them; `walk` proves its own pages by the directories and its own rows.
pub(super) fn keep_ranked(
    (pin, policy, shape): (String, &str, Option<String>),
    (paths, root): (&PathPolicy, &Path),
    walk: &mut Walked,
    entries: Vec<FileSystemEntry>,
    lines: &[Option<Lines>],
    order: &[usize],
) {
    let Some(mut dirs) = memo::walk_proof(root, paths, &walk.meta.walked_dirs, Vec::new()) else {
        return;
    };
    if let super::walked::Verdicts::Proved(verdicts) = &walk.meta.verdicts {
        dirs = dirs.with(verdicts);
    }
    let listed = memo::row_stamps(
        walk.meta.names_only,
        order
            .iter()
            .map(|&at| (Path::new(&entries[at].path), entries[at].stamp)),
    );
    let Some(listed) = listed else {
        return;
    };
    let proof = if listed.is_empty() {
        dirs.clone()
    } else {
        dirs.clone().with_taken(Link::Itself, listed)
    };
    let counted = lines.iter().any(Option::is_some);
    let ranked_lines = if counted {
        order.iter().map(|&at| lines[at]).collect()
    } else {
        Vec::new()
    };
    let mut slots = entries.into_iter().map(Some).collect::<Vec<_>>();
    let entries = order
        .iter()
        .filter_map(|&at| slots[at].take())
        .collect::<Vec<_>>();
    let mut meta = walk.meta.clone();
    meta.walked_dirs = Vec::new();
    walk.meta.walked_dirs = Vec::new();
    walk.dirs = Some(dirs.clone());
    let ranked = Ranked {
        entries,
        lines: ranked_lines,
        meta,
        dirs,
    };
    let bytes = ranked.weight() + proof.weight();
    if let Some(shape) = shape {
        let weight = shape.len() + pin.len();
        FIRSTS.put(shape, policy.to_owned(), weight, pin.clone());
    }
    RANKED.put_proven(pin, policy.to_owned(), bytes, proof, Arc::new(ranked));
}

/// The window `q` pins, from its listing's kept entries while they are
/// current. Its query pinned this walk, whose rows it then cuts as a walk
/// would.
pub(super) fn ranked_window<Q: Listing>(
    q: &Q,
    (root, security): (&Path, &ContentSecurity),
    policy: &str,
    requested: usize,
) -> Option<Walked> {
    if q.early_exit() {
        return None;
    }
    // A first window (no pin yet) is the ranking its shape last kept: a
    // repeat on an unchanged scope reads it instead of walking again.
    let first = q.page() == 1 && q.scan_offset() == 0 && q.walk_offset() == 0;
    let pin = match pinned(q) {
        Some(pin) => pin.to_owned(),
        None if first && q.snapshot().is_none() && serves_firsts() => {
            FIRSTS.get(&shape_key(q, root), policy, Clone::clone)?
        }
        None => return None,
    };
    let ranked = RANKED.current(&pin, policy)?;
    let total = ranked.entries.len();
    let start = q.scan_offset().min(total);
    let end = q.scan_offset().saturating_add(requested).min(total);
    let rows = rows::rows_of(
        &ranked.entries[start..end],
        ranked.lines.get(start..end).unwrap_or_default(),
        (root, security),
        &q.sort(),
    );
    let mut walk = Walked::served(rows, &ranked.meta, q.scan_offset(), ranked.dirs.clone());
    walk.meta.pinned_before = (!first).then(|| pin.clone());
    walk.meta.pins_through = Some(pin);
    rows::group(&mut walk.rows, &q.sort());
    Some(walk)
}

/// Where a walk-order listing stands at the end of a window, and what the walk
/// that continues from there reports with its own.
#[derive(Clone)]
pub(super) struct Carried {
    pub(super) cursor: WalkCursor,
    /// Rows the windows so far listed: the `scanOffset` this continues.
    rows: usize,
    pub(super) digest: ListingDigest,
    /// What the walk left out through the cursor.
    pub(super) left: LeftSoFar,
    /// Each window's directories, rule files and (a view that shows metadata)
    /// rows.
    proofs: Vec<Proof>,
}

impl Carried {
    /// Add what the windows before this walk's counted to what it did.
    pub(super) fn report_with(&self, native: &mut FileSystemQueryResult) {
        native
            .ignored
            .splice(0..0, self.left.ignored.iter().cloned());
        native.skipped += self.left.unreadable.0;
        native.permission_denied += self.left.unreadable.1;
        native.total_discovered += octocode_engine::num::saturating_u32(self.rows);
    }
}

static CURSORS: PageMemo<Carried> = PageMemo::new();

/// Where the windows before `q`'s stand, while nothing they read changed.
pub(super) fn carried<Q: Listing>(q: &Q, policy: &str) -> Option<Carried> {
    let pin = pinned(q).filter(|_| q.early_exit())?;
    let carried = CURSORS
        .get(pin, policy, Carried::clone)
        .filter(|carried| carried.rows == q.scan_offset())?;
    if carried.proofs.iter().all(Proof::holds) {
        return Some(carried);
    }
    CURSORS.evict(pin);
    None
}

/// What a walk-order window keeps beside where its walk stands.
pub(super) struct KeptWindow<'a> {
    /// Rows listed through this window.
    pub(super) rows: usize,
    /// The walk read no row's metadata, so no row has a stamp to watch.
    pub(super) names_only: bool,
    /// What proves a `testCode` scope's verdicts.
    pub(super) verdicts: Option<&'a Proof>,
    /// The walk positions a `testCode` scope left out so far.
    pub(super) test_code: Vec<u32>,
}

impl<'a> KeptWindow<'a> {
    /// The window through `rows`, proved by `verdicts` when something is.
    pub(super) fn of(
        rows: usize,
        names_only: bool,
        verdicts: &'a super::walked::Verdicts,
        test_code: Vec<u32>,
    ) -> Self {
        Self {
            rows,
            names_only,
            verdicts: match verdicts {
                super::walked::Verdicts::Proved(proof) => Some(proof),
                _ => None,
            },
            test_code,
        }
    }
}

/// Keep where a walk-order walk stands at the end of its window, for the
/// window that pins `digest`. Nothing is kept when a row in view has no
/// stamp or a rule file cannot be watched: that window replays.
pub(super) fn keep_cursor<Q: Listing>(
    q: &Q,
    digest: ListingDigest,
    (paths, root): (&PathPolicy, &Path),
    native: &mut FileSystemQueryResult,
    left: &WalkLeft<'_>,
    earlier: Option<&Carried>,
    window: KeptWindow<'_>,
) {
    let KeptWindow {
        rows,
        names_only,
        verdicts,
        test_code,
    } = window;
    let Some(cursor) = native.window_cursor.take() else {
        return;
    };
    let position = cursor.position();
    let mut ended = FileSystemQueryResult {
        tally: native.tally.clone(),
        window_cursor: Some(cursor),
        ..FileSystemQueryResult::default()
    };
    ended.end_at(position);
    let Some(cursor) = ended.cursor.take() else {
        return;
    };
    let listed = memo::row_stamps(
        names_only,
        native
            .entries
            .iter()
            .map(|entry| (Path::new(&entry.path), entry.stamp)),
    );
    let Some(proof) =
        listed.and_then(|listed| memo::walk_proof(root, paths, &native.walked_dirs, listed))
    else {
        return;
    };
    let mut proofs = earlier.map_or_else(Vec::new, |earlier| earlier.proofs.clone());
    proofs.push(match verdicts {
        Some(verdicts) => proof.with(verdicts),
        None => proof,
    });
    let mut so_far = left.so_far(position);
    if let Some(earlier) = earlier {
        so_far
            .ignored
            .splice(0..0, earlier.left.ignored.iter().cloned());
        so_far.unreadable.0 += earlier.left.unreadable.0;
        so_far.unreadable.1 += earlier.left.unreadable.1;
    }
    so_far.ignored.extend(ended.ignored);
    so_far.unreadable.0 += ended.skipped;
    so_far.unreadable.1 += ended.permission_denied;
    so_far.test_code = test_code;
    so_far.test_code.retain(|at| *at <= position);
    let bytes = cursor.weight()
        + so_far.weight()
        + proofs.iter().map(Proof::weight).sum::<usize>()
        + size_of::<Carried>();
    let through = digest.hex();
    let policy = paths.identity();
    let kept = Carried {
        cursor,
        rows,
        digest,
        left: so_far,
        proofs,
    };
    CURSORS.put(through, policy, bytes, kept);
    if let Some(pin) = pinned(q) {
        CURSORS.evict(pin);
    }
}

/// Drop what `snapshot` pins, as eviction or another process would.
#[cfg(test)]
pub(super) fn forget(snapshot: &str) {
    RANKED.evict(snapshot);
    CURSORS.evict(snapshot);
}

#[cfg(test)]
mod tests;
