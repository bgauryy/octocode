//! What a sorted listing keeps across the windows its time budget pauses.
//!
//! A sorted listing (any order but walk order) ranks its whole scope, so a
//! window that ranked only what the walk reached before the budget ran out
//! would list a wrong first page. A paused window lists no row: it keeps the
//! best rows so far through its window's end ([`Standing::entries`], bounded
//! by the rows asked for, not by the scope), how many rows it ranked, what it
//! left out, the digest of every row it walked, and where the walk stands; the
//! window that resumes it (`next.nextPage`, `walkOffset`) walks on from there
//! and ranks its own rows with them. The window that ends the walk lists the
//! same rows a walk that never paused lists, whatever the clock did.
//!
//! What is kept is a candidate: it serves only while every directory the walk
//! read and every kept row are unchanged ([`Standing::proof`]). A miss (another
//! process, an eviction, a change) walks the scope again from its first entry,
//! with no budget spent before the position the query resumes at, so every
//! window still makes progress ([`octocode_engine::types::FileSystemQueryOptions::budget_after`]).
use super::listing::Listing;
use super::rows::{Lines, Tally};
use super::walk_policy::{LeftSoFar, WalkLeft};
use super::windows::ListingDigest;
use octocode_engine::stamp::Proof;
use octocode_engine::types::{FileSystemEntry, FileSystemQueryResult, WalkCursor};
use octocode_tools_shared::page_memo::PageMemo;
use serde_json::json;
use std::mem::size_of;
use std::path::Path;
use std::sync::Arc;

/// What the windows a sorted walk paused at kept.
pub(super) struct Standing {
    /// The best rows so far, in listing order: at most the window's end
    /// (`scanOffset` plus the walk window).
    pub(super) entries: Vec<FileSystemEntry>,
    /// Each kept row's line count, when the walk counted them.
    pub(super) lines: Vec<Option<Lines>>,
    /// Rows ranked through the cursor, the ones past `entries` included.
    pub(super) ranked: usize,
    /// What those rows total (`summary`).
    pub(super) tally: Tally,
    /// Matches the engine counted through the cursor.
    pub(super) discovered: u32,
    /// Every row walked through the cursor, in walk order.
    pub(super) digest: ListingDigest,
    /// What the walk left out through the cursor.
    pub(super) left: LeftSoFar,
    /// Where the walk stands, when the engine left a cursor at its end.
    pub(super) cursor: Option<WalkCursor>,
    /// Every directory and rule file the walk read, and the kept rows.
    proof: Proof,
}

impl Standing {
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
            + self.left.weight()
            + self.cursor.as_ref().map_or(0, WalkCursor::weight)
            + self.proof.weight()
    }

    /// Add what the windows before this walk counted to what it counted.
    pub(super) fn report_with(&self, native: &mut FileSystemQueryResult) {
        native
            .ignored
            .splice(0..0, self.left.ignored.iter().cloned());
        native.skipped += self.left.unreadable.0;
        native.permission_denied += self.left.unreadable.1;
        native.total_discovered = native.total_discovered.saturating_add(self.discovered);
    }

    /// Put the kept rows before the walk's own, which rank after them on a
    /// tie: they come earlier in walk order.
    pub(super) fn lead(&self, entries: &mut Vec<FileSystemEntry>, lines: &mut Vec<Option<Lines>>) {
        entries.splice(0..0, self.entries.iter().cloned());
        let kept_lines = if self.lines.is_empty() {
            vec![None; self.entries.len()]
        } else {
            self.lines.clone()
        };
        lines.splice(0..0, kept_lines);
    }
}

static STANDINGS: PageMemo<Arc<Standing>> = PageMemo::new();

/// What a standing belongs to: the walk and everything that decides its rows
/// and the window they are cut to, the pin of the windows before it, and the
/// walk position it resumes at.
fn key<Q: Listing>(q: &Q, root: &Path, position: u32) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        "structureSearch.standing",
        q.walk_identity(root),
        q.view(),
        q.sort(),
        q.scan_offset(),
        q.requested(),
        q.snapshot(),
        position,
    ]))
}

/// What the window `q` resumes was kept with, while nothing it read changed.
pub(super) fn take<Q: Listing>(q: &Q, root: &Path, policy: &str) -> Option<Arc<Standing>> {
    let position = q.walk_offset();
    if position == 0 {
        return None;
    }
    let key = key(q, root, position);
    let standing = STANDINGS.get(&key, policy, Arc::clone)?;
    if standing.proof.holds() {
        return Some(standing);
    }
    STANDINGS.evict(&key);
    None
}

/// What a paused window walked, to keep for the window that resumes it.
pub(super) struct Paused<'a> {
    /// The walk's entries, the kept rows first, and their line counts.
    pub(super) entries: Vec<FileSystemEntry>,
    pub(super) lines: &'a [Option<Lines>],
    /// The rows in listing order, by index in `entries`.
    pub(super) order: &'a [usize],
    pub(super) ranked: usize,
    pub(super) tally: Tally,
    pub(super) digest: ListingDigest,
    /// The rows a window lists through its end.
    pub(super) window_end: usize,
    /// The walk read no row's metadata.
    pub(super) names_only: bool,
    /// What a `testCode` scope or a filter read, when it was proved.
    pub(super) verdicts: Option<&'a Proof>,
    /// The walk positions a `testCode` scope left out so far.
    pub(super) test_code: Vec<u32>,
}

/// Keep what `paused` walked under the position its walk stopped at.
/// Nothing is kept when a kept row or a directory cannot be watched: the
/// resume walks again.
pub(super) fn keep<Q: Listing>(
    q: &Q,
    (paths, root): (&octocode_runtime_base::policy::path::PathPolicy, &Path),
    native: &mut FileSystemQueryResult,
    (left, earlier): (&WalkLeft<'_>, Option<&Standing>),
    paused: Paused<'_>,
) {
    let position = native.walk_position;
    let top = &paused.order[..paused.window_end.min(paused.order.len())];
    let mut slots = paused.entries.into_iter().map(Some).collect::<Vec<_>>();
    let entries = top
        .iter()
        .filter_map(|&at| slots[at].take())
        .collect::<Vec<_>>();
    let lines = if paused.lines.iter().any(Option::is_some) {
        top.iter().map(|&at| paused.lines[at]).collect()
    } else {
        Vec::new()
    };
    let listed = super::memo::row_stamps(
        paused.names_only,
        entries
            .iter()
            .map(|entry| (Path::new(&entry.path), entry.stamp)),
    );
    let Some(mut proof) =
        listed.and_then(|listed| super::memo::walk_proof(root, paths, &native.walked_dirs, listed))
    else {
        return;
    };
    if let Some(verdicts) = paused.verdicts {
        proof = proof.with(verdicts);
    }
    if let Some(earlier) = earlier {
        proof = proof.with(&earlier.proof);
    }
    let mut so_far = left.so_far(position);
    so_far.ignored = earlier.map_or_else(Vec::new, |earlier| earlier.left.ignored.clone());
    so_far.ignored.extend(native.ignored.iter().cloned());
    let (skipped, denied) = earlier.map_or((0, 0), |earlier| earlier.left.unreadable);
    so_far.unreadable = (skipped + native.skipped, denied + native.permission_denied);
    so_far.test_code = paused.test_code;
    so_far.test_code.retain(|at| *at <= position);
    let standing = Standing {
        entries,
        lines,
        ranked: paused.ranked,
        tally: paused.tally,
        discovered: earlier
            .map_or(0, |earlier| earlier.discovered)
            .saturating_add(native.total_discovered),
        digest: paused.digest,
        left: so_far,
        cursor: native
            .cursor
            .take()
            .filter(|cursor| cursor.position() == position),
        proof,
    };
    STANDINGS.put(
        key(q, root, position),
        paths.identity(),
        standing.weight(),
        Arc::new(standing),
    );
}

/// Drop every standing kept for listings of the workspace `root`, as
/// eviction or another process would; what other tests in the process kept
/// stays.
#[cfg(test)]
pub(super) fn forget_kept(root: &Path) {
    STANDINGS.evict_policy(&octocode_runtime_base::test_support::workspace_policy(root).identity());
}
