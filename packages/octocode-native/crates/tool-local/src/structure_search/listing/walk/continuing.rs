//! Where a listing walk starts: the window it continues, the cursor a time cut
//! kept, or a replay of the entries before its offset.
use super::super::super::windows::{self, Carried, ListingDigest};
use super::super::super::{Resumes, resume};
use super::super::Listing;
use octocode_engine::types::{ReplayedEntries, WalkCursor};
use std::sync::{Arc, Mutex, PoisonError};

pub(super) struct Continuing {
    /// Walk entries before this walk's own.
    pub(super) walk_offset: u32,
    /// Walk entries this walk lists as its own before its time budget starts:
    /// a sorted walk that lost what its paused windows kept redoes them.
    pub(super) budget_after: u32,
    pub(super) kept: Option<resume::Kept>,
    /// Where the walk starts, when something kept says.
    pub(super) resume: Option<WalkCursor>,
    /// With no cursor the walk passes the rows the windows before it listed by
    /// count: what those entries list now is what the query pinned, or not.
    replay: Option<Arc<Mutex<ListingDigest>>>,
}

impl Continuing {
    pub(super) fn of<Q: Listing>(
        q: &Q,
        root: &std::path::Path,
        (carried, ranked): (Option<&Carried>, super::steps::Ranked<'_>),
        resumes: &Resumes<'_>,
    ) -> Self {
        // A sorted walk resumes where its paused windows stand, the rows
        // before it already ranked; with nothing kept it walks from the
        // first entry.
        match ranked {
            super::steps::Ranked::Resumed(standing) => {
                return Self {
                    walk_offset: q.walk_offset(),
                    budget_after: 0,
                    kept: None,
                    resume: standing.cursor.clone(),
                    replay: None,
                };
            }
            super::steps::Ranked::Unkept => {
                return Self {
                    walk_offset: 0,
                    budget_after: q.walk_offset(),
                    kept: None,
                    resume: None,
                    replay: None,
                };
            }
            super::steps::Ranked::WalkOrder => {}
        }
        let walk_offset = carried.map_or(q.walk_offset(), |carried| carried.cursor.position());
        let kept = carried
            .is_none()
            .then(|| resumes.kept(walk_offset))
            .flatten();
        let resume = carried
            .map(|carried| carried.cursor.clone())
            .or_else(|| kept.as_ref().map(|kept| kept.cursor.clone()));
        let replay = (walk_offset > 0 && resume.is_none())
            .then(|| Arc::new(Mutex::new(windows::ListingDigest::new(q, root))));
        Self {
            walk_offset,
            budget_after: 0,
            kept,
            resume,
            replay,
        }
    }

    /// The engine's reader of what a replay passes.
    pub(super) fn replayed(&self) -> Option<ReplayedEntries> {
        self.replay.as_ref().map(ListingDigest::replayed)
    }

    /// The rows the walk passed before its own: where the window it continues
    /// stands, or what its replay found.
    pub(super) fn passed_rows(&self, carried: Option<&Carried>) -> Option<ListingDigest> {
        carried
            .map(|carried| carried.digest.clone())
            .or_else(|| self.kept.as_ref().map(|kept| kept.digest.clone()))
            .or_else(|| {
                self.replay.as_ref().map(|replay| {
                    replay
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone()
                })
            })
    }
}
