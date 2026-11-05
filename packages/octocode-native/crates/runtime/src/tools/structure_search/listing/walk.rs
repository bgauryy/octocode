//! One listing walk: the engine walk under the listing's admission, its
//! line counts, and the rows it ranks, groups and cuts.
use super::super::files::{is_traversal, sort_key};
use super::super::walk_policy::{WalkLeft, WalkPolicy};
use super::super::walked::{Verdicts, Walked};
use super::super::{Resumes, Walking, end_window, rows, walk_error, windows};
use super::Listing;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use crate::security::ContentSecurity;
use continuing::Continuing;
use octocode_contract_types::StructureSearchSortAnyOrderBy as SortKey;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{FileKind, FileSystemQueryOptions};
use test_code_left::TestCodeLeft;

mod continuing;
mod test_code_left;

/// Walk once: rows ranked by the query's sort and cut to `requested`,
/// path-ordered rows grouped by directory, directories with their own group
/// not also listed bare. A probe walk (`walking`) stops at its first
/// `requested` entries, whatever the sort, and admits nothing past its
/// deadline; a listing walk stops at its time budget.
pub(super) fn walk_listing<Q: Listing>(
    q: &Q,
    root: &std::path::Path,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    requested: usize,
    walking: Walking<'_>,
) -> Result<Walked, ToolError> {
    #[cfg(test)]
    LISTING_WALKS.with(|walks| walks.set(walks.get() + 1));
    let bound = walking.probe();
    // Leave out what localSearch leaves out: ignored entries (unless
    // includeIgnored, counted by the walk) and paths the path policy denies
    // (sensitive names, and symlinks out of the allowed roots). Denied
    // entries the listing would otherwise show are counted and reported,
    // never silently dropped.
    let include = q.globs().0;
    let mut left = WalkLeft::new(paths, root, q.default_excludes(), include);
    let policy = WalkPolicy {
        bound,
        probe: q.filter_probe(root)?,
        show_hidden: q.show_hidden(),
        filtered: q.filtered(),
    };
    let sort = q.sort();
    // A window that continues where the last one stands lists its own rows;
    // any other walk passes the rows earlier windows listed (`scanOffset`).
    let carried = walking
        .budget()
        .and_then(|_| windows::carried(q, &paths.identity()));
    if let Some(carried) = &carried {
        left.carry(&carried.left);
    }
    let before = carried.as_ref().map_or(0, |_| q.scan_offset());
    let passed = q.scan_offset() - before;
    let window_end = passed.saturating_add(requested);
    let resumes = Resumes {
        identity: q.walk_identity(root),
        paths,
        root,
    };
    let continuing = Continuing::of(q, root, walking, carried.as_ref(), &resumes);
    let test_code = TestCodeLeft::of(q.test_code(), paths);
    if let Some((test_code, carried)) = test_code.as_ref().zip(carried.as_ref()) {
        test_code.carry(&carried.left.test_code);
    }
    let started = octocode_engine::stamp::settle_clock();
    let names_only = reads_names_only(q, &sort, bound.is_some());
    let engine = q.engine_options(root);
    // An access time moves with no stamp to show it, so only a walk tells.
    let watched = engine.accessed_from.is_none() && engine.accessed_to.is_none();
    let mut native = octocode_engine::search::fs_query::query_file_system(
        FileSystemQueryOptions {
            resume: continuing.resume.clone(),
            replayed: continuing.replayed(),
            window_rows: walking.budget().map(|_| saturating_u32(window_end)),
            time_budget: walking.budget(),
            // The callback applies the default prune (`WalkLeft`).
            stop_at_limit: Some(true),
            skip_entries: Some(continuing.walk_offset),
            limit: walk_limit(q, bound.is_some(), requested, window_end),
            lists: test_code
                .as_ref()
                .map(|left| left.rule(continuing.walk_offset)),
            names_only,
            ..engine
        },
        &|path, file_type, visit| left.admit(&policy, path, file_type, visit),
        &|| cancel.check().is_err(),
    )
    .map_err(|error| walk_error(error, q.path(), cancel))?;
    cancel.check().map_err(ToolError::cancelled)?;
    let mut lines = line_counts(&mut native, &sort, paths, cancel, walking)?;
    let window = end_window(&mut native, window_end, passed, is_traversal(&sort));
    // `end_window` keeps a walk-order prefix: the counts of its entries.
    lines.truncate(native.entries.len());
    let mut ranking = rows::Ranking::of(&native.entries, &lines, (root, security), &sort);
    let available = before + ranking.order().len();
    // A walk cut by time ranks only what it reached, so it pins nothing.
    let current = walking.budget().is_some() && native.time_cut.is_none();
    let continues = current
        && if q.early_exit() {
            native.was_capped
        } else {
            available > q.scan_offset().saturating_add(requested)
        };
    let pins = windows::pins(
        q,
        root,
        (&native.entries, &lines, &ranking),
        windows::PinnedWalk {
            passed: continuing.passed_rows(carried.as_ref()),
            carried: carried.is_some(),
            listing: walking.budget().is_some(),
            time_cut: native.time_cut.is_some(),
            continues,
        },
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
    let keepable = !matches!(verdicts, Verdicts::Unproved);
    resumes.keep(
        &mut native,
        walking,
        (pins.through.as_ref(), names_only, &verdicts),
    );
    if let Some(digest) = pins
        .through
        .filter(|_| continues && q.early_exit() && keepable)
    {
        let test_code = test_code
            .as_ref()
            .map_or_else(Vec::new, TestCodeLeft::positions);
        windows::keep_cursor(
            q,
            digest,
            (paths, root),
            &mut native,
            &left,
            carried.as_ref(),
            windows::KeptWindow::of(
                requested + q.scan_offset(),
                names_only,
                &verdicts,
                test_code,
            ),
        );
    }
    let rows = ranking.rows(
        &native.entries,
        &lines,
        (root, security),
        passed..window_end,
    );
    if let Some(carried) = &carried {
        carried.report_with(&mut native);
    }
    let entries = std::mem::take(&mut native.entries);
    let mut walk = Walked::finish(
        native,
        rows,
        available,
        q.scan_offset(),
        left,
        security,
        names_only,
    );
    walk.meta.time_window = window;
    walk.meta.pinned_before = pins.before;
    walk.meta.pins_through.clone_from(&through);
    walk.meta.verdicts = verdicts;
    walk.meta.test_code = test_code.map(|left| left.window(walk.meta.walk_position));
    if let Some(through) = through.filter(|_| continues && !q.early_exit() && keepable) {
        windows::keep_ranked(
            (through, &paths.identity()),
            (paths, root),
            &mut walk,
            entries,
            &lines,
            ranking.order(),
        );
    }
    rows::group(&mut walk.rows, &sort, q.view());
    Ok(walk)
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
    probe: bool,
) -> bool {
    probe
        || (q.view() == octocode_contract_types::StructureSearchView::Paths
            && matches!(sort_key(sort), None | Some(SortKey::Name | SortKey::Path)))
}

/// A probe stops at its own entry cap, and a walk-order listing at the first
/// match past its window (the first matches are final once found); any other
/// listing walks its whole scope, bounded only by time, so its sort ranks every
/// entry.
fn walk_limit<Q: Listing>(q: &Q, probe: bool, requested: usize, window_end: usize) -> Option<u32> {
    if probe {
        Some(saturating_u32(requested))
    } else if q.early_exit() {
        Some(saturating_u32(window_end))
    } else {
        None
    }
}

/// The line sort's key, counted for every file it ranks (a probe only counts
/// entries and ranks none) within the walk's time budget, measured from the
/// count's start so a walk that spent its own budget still ranks what it
/// walked. Counting is part of the walk: a budget spent before every file is
/// counted cuts the walk at the first uncounted one ([`cut_walk_before`]), and
/// `next.nextPage` resumes there.
fn line_counts(
    native: &mut octocode_engine::types::FileSystemQueryResult,
    sort: &octocode_contract_types::StructureSearchSort,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    walking: Walking<'_>,
) -> Result<Vec<Option<rows::Lines>>, ToolError> {
    if sort_key(sort) != Some(SortKey::LineCount) || walking.probe().is_some() {
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
        walking
            .budget()
            .map(|budget| octocode_engine::scan_budget::walk_now() + budget),
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
