//! Named steps of one listing walk: the engine walk it asks for, the pins of
//! its rows, and what a sorted walk its budget paused keeps.
use super::super::super::Resumes;
use super::super::super::files::is_traversal;
use super::super::super::standing::{self, Standing};
use super::super::super::walk_policy::{LeftSoFar, WalkLeft, WalkPolicy};
use super::super::super::walked::Verdicts;
use super::super::super::windows::{self, Carried, ListingDigest, Pins};
use super::super::super::{rows, walk_error};
use super::super::Listing;
use super::TestCodeLeft;
use super::continuing::Continuing;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{FileSystemEntry, FileSystemQueryOptions, FileSystemQueryResult};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::result::ToolError;

/// Where a listing walk stands in its ranking.
#[derive(Clone, Copy)]
pub(super) enum Ranked<'a> {
    /// Walk order (a `tree`, `files` by traversal) or a probe.
    WalkOrder,
    /// A sorted walk with nothing kept before it: its first window, or one
    /// whose paused windows' standing was lost.
    Unkept,
    /// A sorted walk resuming the windows its budget paused.
    Resumed(&'a Standing),
}

impl<'a> Ranked<'a> {
    pub(super) fn sorted(self) -> bool {
        !matches!(self, Self::WalkOrder)
    }

    pub(super) fn standing(self) -> Option<&'a Standing> {
        match self {
            Self::Resumed(standing) => Some(standing),
            Self::WalkOrder | Self::Unkept => None,
        }
    }
}

/// What a listing walk starts from: what the windows before it kept.
pub(super) struct Start {
    /// A sorted listing ranks its whole scope ([`standing`]).
    sorted: bool,
    pub(super) standing: Option<std::sync::Arc<Standing>>,
    /// Where the walk-order window before it stands.
    pub(super) carried: Option<Carried>,
    /// Rows the windows before a carried one listed.
    pub(super) before: usize,
    /// Rows of the walk's own ranking before this window (`scanOffset`).
    pub(super) passed: usize,
    pub(super) window_end: usize,
}

impl Start {
    pub(super) fn of<Q: Listing>(
        q: &Q,
        (root, paths): (&std::path::Path, &PathPolicy),
        requested: usize,
    ) -> Self {
        let sorted = !is_traversal(&q.sort());
        let standing = sorted
            .then(|| standing::take(q, root, &paths.identity()))
            .flatten();
        // A window that continues where the last one stands lists its own
        // rows; any other walk passes the rows earlier windows listed.
        let carried = windows::carried(q, &paths.identity());
        let before = carried.as_ref().map_or(0, |_| q.scan_offset());
        let passed = q.scan_offset() - before;
        Self {
            sorted,
            standing,
            carried,
            before,
            passed,
            window_end: passed.saturating_add(requested),
        }
    }

    /// The walk's time budget. A later page of a sorted window re-ranks the
    /// window its first page ended, whole: a pause there would cut other rows.
    pub(super) fn budget<Q: Listing>(
        &self,
        q: &Q,
        budget: std::time::Duration,
    ) -> Option<std::time::Duration> {
        (!(self.sorted && q.page() > 1)).then_some(budget)
    }

    /// Add what the windows before this walk counted to what it counted; a
    /// paused sorted walk hands them on instead ([`standing::keep`]).
    pub(super) fn report_with(&self, native: &mut FileSystemQueryResult, paused: bool) {
        if let Some(carried) = &self.carried {
            carried.report_with(native);
        }
        if let Some(standing) = self.standing.as_ref().filter(|_| !paused) {
            standing.report_with(native);
        }
    }

    pub(super) fn ranked(&self) -> Ranked<'_> {
        match (&self.standing, self.sorted) {
            (Some(standing), _) => Ranked::Resumed(standing),
            (None, true) => Ranked::Unkept,
            (None, false) => Ranked::WalkOrder,
        }
    }

    /// What the windows before this walk left out: the walk-order window it
    /// continues, or the paused windows of a sorted walk.
    pub(super) fn so_far(&self) -> Option<&LeftSoFar> {
        self.carried
            .as_ref()
            .map(|carried| &carried.left)
            .or(self.standing.as_ref().map(|standing| &standing.left))
    }
}

/// What one listing walk asks the engine for beyond the query's filters.
pub(super) struct EngineWalk<'a> {
    pub(super) continuing: &'a Continuing,
    pub(super) test_code: Option<&'a TestCodeLeft>,
    /// Rows through this window's end, for a listing walk's window cursor.
    pub(super) window_rows: Option<usize>,
    pub(super) limit: Option<u32>,
    pub(super) names_only: bool,
    pub(super) budget: Option<std::time::Duration>,
    /// The global ignore rules of the call.
    pub(super) global_ignore: octocode_engine::search::walk::GlobalIgnore,
}

/// The engine walk of `q` over `root` under the listing's admission.
pub(super) fn run_engine<Q: Listing>(
    q: &Q,
    root: &std::path::Path,
    walk: EngineWalk<'_>,
    (left, policy): (&WalkLeft<'_>, &WalkPolicy),
    cancel: &dyn CancellationCheck,
) -> Result<FileSystemQueryResult, ToolError> {
    let EngineWalk {
        continuing,
        test_code,
        window_rows,
        limit,
        names_only,
        budget,
        global_ignore,
    } = walk;
    let native = octocode_engine::search::fs_query::query_file_system(
        FileSystemQueryOptions {
            resume: continuing.resume.clone(),
            replayed: continuing.replayed(),
            window_rows: window_rows.map(saturating_u32),
            time_budget: budget,
            // The callback applies the default prune (`WalkLeft`).
            stop_at_limit: Some(true),
            skip_entries: Some(continuing.walk_offset),
            budget_after: Some(continuing.budget_after),
            limit,
            lists: test_code.map(|left| left.rule(continuing.walk_offset)),
            names_only,
            global_ignore,
            ..q.engine_options(root)
        },
        &|path, file_type, visit| left.admit(policy, path, file_type, visit),
        &|| cancel.check().is_err(),
    )
    .map_err(|error| walk_error(error, q.path(), cancel))?;
    cancel.check().map_err(ToolError::cancelled)?;
    Ok(native)
}

/// What this window pins ([`windows::pins`] for a walk-order listing,
/// [`windows::ranked_pins`] for a sorted one), and the digest a paused
/// sorted walk hands on. `time_cut`: the walk stopped at its time budget;
/// `continues`: a window follows this one by row count.
pub(super) fn window_pins<Q: Listing>(
    q: &Q,
    root: &std::path::Path,
    walked: (&[FileSystemEntry], &[Option<rows::Lines>], &rows::Ranking),
    (start, continuing): (&Start, &Continuing),
    (time_cut, continues): (bool, bool),
) -> (Pins, Option<ListingDigest>) {
    let Some(standing) = start.sorted.then_some(start.standing.as_deref()) else {
        let walk = windows::PinnedWalk {
            passed: continuing.passed_rows(start.carried.as_ref()),
            carried: start.carried.is_some(),
            listing: true,
            time_cut,
            continues,
        };
        return (windows::pins(q, root, walked, walk), None);
    };
    windows::ranked_pins(
        q,
        walked,
        (
            standing.map(|standing| standing.digest.clone()),
            standing.map_or(0, |standing| standing.entries.len()),
        ),
        (time_cut, continues),
        root,
    )
}

/// What a window keeps for the one after it.
pub(super) struct Keeping<'a> {
    pub(super) pins: &'a Pins,
    /// The digest of every row a paused sorted walk walked.
    pub(super) running: Option<ListingDigest>,
    pub(super) verdicts: &'a Verdicts,
    pub(super) names_only: bool,
    pub(super) continues: bool,
    pub(super) paused: bool,
    pub(super) requested: usize,
    /// Rows the walk ranked, the windows' before it included.
    pub(super) available: usize,
    pub(super) lines: &'a [Option<rows::Lines>],
    pub(super) order: &'a [usize],
    /// What the rows the walk ranked total, the windows' before it included.
    pub(super) tally: rows::Tally,
    pub(super) test_code: Option<&'a TestCodeLeft>,
}

/// Keep where this window stands for the window after it: a walk-order walk
/// cut by time its cursor ([`Resumes`]), one cut at its window its walk
/// ([`windows::keep_cursor`]), a paused sorted walk what it ranked
/// ([`standing`]). Nothing is kept when its verdicts have no proof.
pub(super) fn keep_window<Q: Listing>(
    q: &Q,
    (paths, root, resumes): (&PathPolicy, &std::path::Path, &Resumes<'_>),
    native: &mut FileSystemQueryResult,
    (left, start): (&WalkLeft<'_>, &Start),
    keeping: Keeping<'_>,
) {
    let keepable = !matches!(keeping.verdicts, Verdicts::Unproved);
    let positions = || {
        keeping
            .test_code
            .map_or_else(Vec::new, TestCodeLeft::positions)
    };
    if !start.sorted {
        resumes.keep(
            native,
            (
                keeping.pins.through.as_ref(),
                keeping.names_only,
                keeping.verdicts,
            ),
        );
    }
    if let Some(digest) = keeping
        .pins
        .through
        .clone()
        .filter(|_| keeping.continues && q.early_exit() && keepable)
    {
        windows::keep_cursor(
            q,
            digest,
            (paths, root),
            native,
            left,
            start.carried.as_ref(),
            windows::KeptWindow::of(
                keeping.requested + q.scan_offset(),
                keeping.names_only,
                keeping.verdicts,
                positions(),
            ),
        );
    }
    let Some(digest) = keeping.running.filter(|_| keeping.paused && keepable) else {
        return;
    };
    let entries = std::mem::take(&mut native.entries);
    standing::keep(
        q,
        (paths, root),
        native,
        (left, start.standing.as_deref()),
        standing::Paused {
            entries,
            lines: keeping.lines,
            order: keeping.order,
            ranked: keeping.available,
            tally: keeping.tally,
            digest,
            window_end: start.window_end,
            names_only: keeping.names_only,
            verdicts: match keeping.verdicts {
                Verdicts::Proved(proof) => Some(proof),
                Verdicts::Unscoped | Verdicts::Unproved => None,
            },
            test_code: positions(),
        },
    );
}
