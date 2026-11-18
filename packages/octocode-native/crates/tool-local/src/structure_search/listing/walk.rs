//! One listing walk: the engine walk under the listing's admission, its
//! line counts, and the rows it ranks, groups and cuts.
use super::super::cut::TimeWindow;
use super::super::files::sort_key;
use super::super::walk_policy::{WalkLeft, WalkPolicy};
use super::super::walked::{Verdicts, Walked};
use super::super::{Resumes, end_window, rows, windows};
use super::Listing;
use continuing::Continuing;
use octocode_contract_types::StructureSearchSortAnyOrderBy as SortKey;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::FileKind;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::security::ContentSecurity;
use test_code_left::TestCodeLeft;

mod continuing;
mod steps;
mod test_code_left;

/// Walk once: rows ranked by the query's sort and cut to `requested`,
/// path-ordered rows grouped by directory, directories with their own group
/// not also listed bare. The walk stops at its time budget.
pub(super) fn walk_listing<Q: Listing>(
    q: &Q,
    root: &std::path::Path,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    requested: usize,
    listing_budget: std::time::Duration,
) -> Result<Walked, ToolError> {
    #[cfg(test)]
    LISTING_WALKS.with(|walks| walks.set(walks.get() + 1));
    // Leave out what localSearch leaves out: ignored entries (unless
    // includeIgnored, counted by the walk) and paths the path policy denies
    // (sensitive names, and symlinks out of the allowed roots). Denied
    // entries the listing would otherwise show are counted and reported,
    // never silently dropped.
    let include = q.globs().0;
    let mut left = WalkLeft::new(paths, root, q.default_excludes(), include);
    let policy = WalkPolicy {
        probe: q.filter_probe(root)?,
        show_hidden: q.show_hidden(),
        filtered: q.filtered(),
    };
    let sort = q.sort();
    let start = steps::Start::of(q, (root, paths), requested);
    let ranked = start.ranked();
    if let Some(earlier) = start.so_far() {
        left.carry(earlier);
    }
    let resumes = Resumes {
        identity: q.walk_identity(root),
        paths,
        root,
    };
    let continuing = Continuing::of(q, root, (start.carried.as_ref(), ranked), &resumes);
    let test_code = TestCodeLeft::of(q.test_code(), paths);
    if let Some((test_code, earlier)) = test_code.as_ref().zip(start.so_far()) {
        test_code.carry(&earlier.test_code);
    }
    let started = octocode_engine::stamp::settle_clock();
    let names_only = reads_names_only(q, &sort);
    let engine = q.engine_options(root);
    // An access time moves with no stamp to show it, so only a walk tells.
    let watched = engine.accessed_from.is_none() && engine.accessed_to.is_none();
    let budget = start.budget(q, listing_budget);
    let window_end = start.window_end;
    let mut native = steps::run_engine(
        q,
        root,
        steps::EngineWalk {
            continuing: &continuing,
            test_code: test_code.as_ref(),
            window_rows: Some(window_end),
            limit: walk_limit(q, window_end),
            names_only,
            budget,
            global_ignore: paths.global_ignore(),
        },
        (&left, &policy),
        cancel,
    )?;
    let mut lines = line_counts(&mut native, &sort, (paths, cancel), budget)?;
    let window = if ranked.sorted() {
        TimeWindow::Whole
    } else {
        end_window(&mut native, window_end)
    };
    // `end_window` keeps a walk-order prefix: the counts of its entries.
    lines.truncate(native.entries.len());
    let standing = ranked.standing();
    let kept = standing.map_or(0, |standing| standing.entries.len());
    if let Some(standing) = standing {
        standing.lead(&mut native.entries, &mut lines);
    }
    let mut ranking = rows::Ranking::of(&native.entries, &lines, (root, security), &sort);
    let paused = ranked.sorted() && native.time_cut.is_some();
    // Every row the walk ranked: its own entries follow the rows a standing
    // kept, which the standing's tally counts with the rest its paused windows
    // ranked. Only a walk of its whole scope ranked every row of the listing.
    let tally = rows::Tally::of_entries(
        &native.entries,
        ranking.order().iter().copied().filter(|&at| at >= kept),
    )
    .with(standing.map_or_else(rows::Tally::default, |standing| standing.tally));
    let whole_scope = !q.early_exit() && native.time_cut.is_none();
    let available =
        ranking.order().len() - kept + standing.map_or(start.before, |standing| standing.ranked);
    // A walk cut by time ranks only what it reached, so it pins nothing.
    let current = native.time_cut.is_none();
    let continues = current
        && if q.early_exit() {
            native.was_capped
        } else {
            available > q.scan_offset().saturating_add(requested)
        };
    let (pins, running) = steps::window_pins(
        q,
        root,
        (&native.entries, &lines, &ranking),
        (&start, &continuing),
        (native.time_cut.is_some(), continues),
    );
    let through = pins.pin_through();
    // What a `testCode` scope's verdicts read, a parent module outside this
    // root too, what a metadata filter or a replay read of the entries it
    // judged, and what the windows before a kept cursor read prove a kept listing
    // as its directories do.
    let verdicts = verdicts_of(
        test_code.as_ref().map(|left| left.scoped.proof(started)),
        std::mem::take(&mut native.evaluated),
        continuing.kept.as_ref().map(|kept| &kept.proof),
        watched,
    );
    steps::keep_window(
        q,
        (paths, root, &resumes),
        &mut native,
        (&left, &start),
        steps::Keeping {
            pins: &pins,
            running,
            verdicts: &verdicts,
            names_only,
            continues,
            paused,
            requested,
            available,
            lines: &lines,
            order: ranking.order(),
            tally,
            test_code: test_code.as_ref(),
        },
    );
    // A paused sorted walk lists no row: its rows come when the walk ends.
    let rows = if paused {
        Vec::new()
    } else {
        let window = start.passed..window_end;
        ranking.rows(&native.entries, &lines, (root, security), window)
    };
    start.report_with(&mut native, paused);
    let entries = std::mem::take(&mut native.entries);
    let shown = if paused { 0 } else { available };
    let mut walk = Walked::finish(
        native,
        rows,
        shown,
        q.scan_offset(),
        left,
        security,
        names_only,
    );
    walk.meta.time_window = if paused { TimeWindow::Paused } else { window };
    walk.meta.ranked = whole_scope.then_some(tally);
    walk.meta.pinned_before = pins.before;
    // A paused window's resume carries the pin of the windows before it.
    walk.meta.pins_through = if paused {
        q.snapshot().map(str::to_owned)
    } else {
        through.clone()
    };
    let keepable = !matches!(verdicts, Verdicts::Unproved);
    walk.meta.verdicts = verdicts;
    walk.meta.test_code = test_code.map(|left| left.window(walk.meta.walk_position));
    // Only a walk that ranked every row itself holds the whole listing.
    if continues && keepable && !q.early_exit() && standing.is_none() {
        // Only a walk that ran its scope from the start in one window is what
        // a repeat of the first window would walk: a resumed walk's ranking
        // carries only its own part of what the walk left out.
        let first = q.walk_offset() == 0 && q.scan_offset() == 0 && q.snapshot().is_none();
        keep_whole(
            through.map(|through| (through, first.then(|| windows::shape_key(q, root)))),
            (paths, root),
            &mut walk,
            (entries, &lines, ranking.order()),
        );
    }
    rows::group(&mut walk.rows, &sort);
    Ok(walk)
}

/// Keep a listing the walk ranked whole, past one window, for the windows
/// after its first ([`windows::keep_ranked`]).
fn keep_whole(
    through: Option<(String, Option<String>)>,
    (paths, root): (&PathPolicy, &std::path::Path),
    walk: &mut Walked,
    (entries, lines, order): (
        Vec<octocode_engine::types::FileSystemEntry>,
        &[Option<rows::Lines>],
        &[usize],
    ),
) {
    if let Some((through, shape)) = through {
        windows::keep_ranked(
            (through, &paths.identity(), shape),
            (paths, root),
            walk,
            entries,
            lines,
            order,
        );
    }
}

/// What a `testCode` scope read, when a proof of it could be taken.
type ScopeProof = Option<octocode_engine::stamp::Proof>;

/// [`Verdicts::of`] a walk whose `testCode` scope, if any, may have no proof.
fn verdicts_of(
    scoped: Option<ScopeProof>,
    evaluated: Vec<octocode_engine::types::EvaluatedEntry>,
    earlier: Option<&octocode_engine::stamp::Proof>,
    watched: bool,
) -> Verdicts {
    let proved = !matches!(scoped, Some(None));
    Verdicts::of(scoped.flatten(), evaluated, earlier, watched && proved)
}

/// A probe only counts entries, and a paths view ranked by name shows nothing
/// else: neither reads an entry's metadata. Sizes, times and line counts rank
/// other sorts, and what ranks a page must be watched.
fn reads_names_only<Q: Listing>(
    q: &Q,
    sort: &octocode_contract_types::StructureSearchSort,
) -> bool {
    q.view() == octocode_contract_types::StructureSearchView::Paths
        && matches!(sort_key(sort), None | Some(SortKey::Name | SortKey::Path))
}

/// A walk-order listing stops at the first match past its window (the first
/// matches are final once found); any other listing walks its whole scope,
/// bounded only by time, so its sort ranks every entry.
fn walk_limit<Q: Listing>(q: &Q, window_end: usize) -> Option<u32> {
    q.early_exit().then(|| saturating_u32(window_end))
}

/// The line sort's key, counted for every file it ranks within the walk's
/// time `budget`, measured from the
/// count's start so a walk that spent its own budget still ranks what it
/// walked. Counting is part of the walk: a budget spent before every file is
/// counted cuts the walk at the first uncounted one ([`cut_walk_before`]), and
/// `next.nextPage` resumes there.
fn line_counts(
    native: &mut octocode_engine::types::FileSystemQueryResult,
    sort: &octocode_contract_types::StructureSearchSort,
    (paths, cancel): (&PathPolicy, &dyn CancellationCheck),
    budget: Option<std::time::Duration>,
) -> Result<Vec<Option<rows::Lines>>, ToolError> {
    if sort_key(sort) != Some(SortKey::TotalLines) {
        return Ok(vec![None; native.entries.len()]);
    }
    let (lines, cut) = rows::count_lines(
        &native
            .entries
            .iter()
            .map(|e| (e.entry_type != FileKind::Directory).then(|| std::path::Path::new(&e.path)))
            .collect::<Vec<_>>(),
        paths,
        cancel,
        budget.map(|budget| octocode_engine::scan_budget::walk_now() + budget),
    )?;
    if let Some(at) = cut {
        cut_walk_before(native, at);
    }
    Ok(lines)
}

/// End the walk before `native.entries[at]`, as its time budget does: the
/// entry and every one after it in walk order are uninspected, and
/// `next.nextPage` resumes there.
fn cut_walk_before(native: &mut octocode_engine::types::FileSystemQueryResult, at: usize) {
    let Some(first) = native.entries.get(at) else {
        return;
    };
    let (relative, before) = (
        first.relative_path.clone(),
        first.walk_position.saturating_sub(1),
    );
    // The window ends before the walk's own end and counts only what it
    // covers.
    native.end_at(before);
    native.time_cut = Some(relative);
}

#[cfg(test)]
thread_local! {
    static LISTING_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Listing and probe walks this thread ran.
#[cfg(test)]
pub(crate) fn listing_walks() -> usize {
    LISTING_WALKS.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests;
