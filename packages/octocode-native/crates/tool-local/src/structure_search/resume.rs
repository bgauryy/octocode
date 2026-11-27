//! Where a listing walk cut by time stands, kept for the window that resumes
//! it (`next.nextPage`, `walkOffset`): the directories the walk is
//! inside, with the entries still to inspect, so the resumed walk starts at
//! its first new entry and replays nothing (replaying a deep offset costs
//! most of a window's budget).
//!
//! `walkOffset` is a count, so it reads as the same place only over the same
//! entries. The resume's `snapshot` pins the rows listed through the cut, in
//! walk order ([`ListingDigest`]). A kept cursor is a candidate, never trusted
//! alone: it serves only while the engine accepts it (the same walk shape,
//! exactly at the resume position), every directory it holds keeps the
//! modification time its listing was read at, and all the windows before it
//! read still holds ([`Kept::proof`]). Any other case replays: an entry created,
//! removed or renamed, an edited rule, an expired or evicted cursor, or another
//! process. The replay judges the entries it passes as the first walk did and
//! hashes what they list now: the same rows mean no row moved across the cut,
//! so the walk goes on and lists every other row once; other rows restart the
//! listing (`staleSnapshot`).
use super::memo::{self, Seen};
use super::walked::Verdicts;
use super::windows::ListingDigest;
use octocode_engine::stamp::Proof;
use octocode_engine::types::{FileSystemQueryResult, WalkCursor};
use octocode_tools_shared::page_memo::PageMemo;
use serde_json::{Value, json};

/// What a window cut by time leaves for the one that resumes it.
#[derive(Clone)]
pub(super) struct Kept {
    /// What every window so far read and the cursor's directories.
    pub(super) proof: Proof,
    pub(super) cursor: WalkCursor,
    /// The rows listed through the cursor.
    pub(super) digest: ListingDigest,
}

/// Kept cursors, weighed by the bytes they hold against the one process
/// budget ([`octocode_engine::memory::MEMORY`]), the oldest dropped first.
static STORE: PageMemo<Kept> = PageMemo::new();

/// What the cursor belongs to: the walk (`identity`: the root and everything
/// that decides its order and what it enters) and the position it resumes at.
fn key(identity: &Value, position: u32) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        "structureSearch.walkCursor",
        identity,
        position
    ]))
}

/// How a listing walk keeps and uses its cursor: the walk's identity and the
/// path policy it ran under, and its root.
pub(super) struct Resumes<'a> {
    pub(super) identity: Value,
    pub(super) paths: &'a octocode_runtime_base::policy::path::PathPolicy,
    pub(super) root: &'a std::path::Path,
}

impl Resumes<'_> {
    /// What the window a listing walk resumes at `walk_offset` left, when
    /// nothing it depends on changed.
    pub(super) fn kept(&self, walk_offset: u32) -> Option<Kept> {
        (walk_offset > 0)
            .then(|| take(&self.identity, &self.paths.identity(), walk_offset))
            .flatten()
    }

    /// Keep where the walk stands at the end of its window, for the window
    /// that resumes there. Rows it has no stamp for, or a read no proof can
    /// watch, keep nothing: that window replays.
    pub(super) fn keep(
        &self,
        native: &mut FileSystemQueryResult,
        (digest, names_only, verdicts): (Option<&ListingDigest>, bool, &Verdicts),
    ) {
        if native.time_cut.is_none() {
            return;
        }
        let Some(cursor) = native
            .cursor
            .take()
            .filter(|cursor| cursor.position() == native.walk_position)
        else {
            return;
        };
        let Some(digest) = digest else {
            return;
        };
        let listed = memo::row_stamps(
            names_only,
            native
                .entries
                .iter()
                .map(|entry| (std::path::Path::new(&entry.path), entry.stamp)),
        );
        let Some(walked) = listed.and_then(|listed| {
            memo::walk_proof(self.root, self.paths, &native.walked_dirs, listed)
        }) else {
            return;
        };
        let read = match verdicts {
            Verdicts::Unscoped => walked,
            Verdicts::Proved(verdicts) => walked.with(verdicts),
            Verdicts::Unproved => return,
        };
        keep(
            &self.identity,
            (&self.paths.identity(), self.paths),
            self.root,
            cursor,
            (digest.clone(), read),
        );
    }
}

/// Keep `cursor`, where a walk of `root` stands, for the walk that resumes at
/// its position, with the rows listed through it and what all the windows so far
/// `read`. Nothing is kept when a directory it holds cannot be stamped.
fn keep(
    identity: &Value,
    (policy, paths): (&str, &octocode_runtime_base::policy::path::PathPolicy),
    root: &std::path::Path,
    cursor: WalkCursor,
    (digest, read): (ListingDigest, Proof),
) {
    let directories = cursor.listed_directories();
    let held = directories.iter().map(|(dir, _)| *dir).collect::<Vec<_>>();
    let listed = directories
        .iter()
        .map(|(path, modified)| {
            modified.map(|modified| Seen {
                path,
                size: 0,
                modified: Some(modified),
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(proof) = listed.and_then(|listed| memo::proof(root, paths, listed, &held)) else {
        return;
    };
    let proof = proof.with(&read);
    let (key, weight) = (
        key(identity, cursor.position()),
        cursor.weight() + proof.weight(),
    );
    STORE.put(
        key,
        policy.to_owned(),
        weight,
        Kept {
            proof,
            cursor,
            digest,
        },
    );
}

/// What was kept for the walk that resumes at `position`, while nothing it
/// depends on changed; one that did is dropped.
fn take(identity: &Value, policy: &str, position: u32) -> Option<Kept> {
    let key = key(identity, position);
    let kept = STORE.get(&key, policy, Kept::clone)?;
    if kept.proof.holds() {
        return Some(kept);
    }
    STORE.evict(&key);
    None
}

/// Drop every cursor kept for listings of the workspace `root`, as eviction
/// or another process would; what other tests in the process kept stays.
#[cfg(test)]
pub(super) fn forget_kept(root: &std::path::Path) {
    STORE.evict_policy(&octocode_runtime_base::test_support::workspace_policy(root).identity());
}

#[cfg(test)]
mod tests;
