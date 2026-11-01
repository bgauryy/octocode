//! One listing walk: the engine walk under the listing's admission, its
//! line counts, and the rows it ranks, groups and cuts.
use super::super::files::{is_traversal, sort_key};
use super::super::walk_policy::{WalkLeft, WalkPolicy};
use super::super::walked::Walked;
use super::super::{Resumes, Walking, end_window, rows, walk_error};
use super::Listing;
use crate::execution::cancel::CancellationCheck;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use crate::security::ContentSecurity;
use octocode_contract_types::StructureSearchQueryFilesSortVariant0By as SortKey;
use octocode_engine::types::{FileKind, FileSystemQueryOptions};

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
    let left = WalkLeft::new(paths, root, q.default_excludes(), include);
    let policy = WalkPolicy {
        bound,
        probe: q.filter_probe(root)?,
        show_hidden: q.show_hidden(),
        filtered: q.filtered(),
    };
    let sort = q.sort();
    // The rows this window ends at: the earlier windows' (`scanOffset`)
    // and its own `requested`.
    let window_end = q.scan_offset().saturating_add(requested);
    // A probe stops at its own entry cap, and a walk-order listing at the
    // first match past its window (the first matches are final once found);
    // any other listing walks its whole scope, bounded only by time, so
    // its sort ranks every entry.
    let walk_limit = if bound.is_some() {
        Some(u32_of_usize(requested))
    } else if q.early_exit() {
        Some(u32_of_usize(window_end))
    } else {
        None
    };
    let resumes = Resumes {
        identity: q.walk_identity(root),
        paths,
        root,
    };
    let (resume, window_rows) = resumes.options(walking, q.walk_offset(), window_end);
    let test_code = TestCodeLeft::of(q.test_code(), paths);
    let mut native = octocode_engine::search::fs_query::query_file_system(
        FileSystemQueryOptions {
            resume,
            window_rows,
            time_budget: walking.budget(),
            // The callback applies the default prune (`WalkLeft`).
            exclude_dir: None,
            stop_at_limit: Some(true),
            skip_entries: Some(q.walk_offset()),
            limit: walk_limit,
            lists: test_code.as_ref().map(TestCodeLeft::rule),
            ..q.engine_options(root)
        },
        &|path, file_type, visit| left.admit(&policy, path, file_type, visit),
        &|| cancel.check().is_err(),
    )
    .map_err(|error| walk_error(error, q.path(), cancel))?;
    cancel.check().map_err(ToolError::cancelled)?;
    // The line sort's key, counted for every file it ranks (a probe only
    // counts entries and ranks none) within the walk's time budget, measured
    // from the count's start so a walk that spent its own budget still ranks
    // what it walked. Counting is part of the walk: a budget spent before
    // every file is counted cuts the walk at the first uncounted one
    // ([`cut_walk_before`]), and `next.continueWalk` resumes there.
    let mut lines = if sort_key(&sort) == Some(SortKey::LineCount) && bound.is_none() {
        let (lines, cut) = rows::count_lines(
            &native
                .entries
                .iter()
                .map(|e| {
                    (e.entry_type != FileKind::Directory).then(|| std::path::Path::new(&e.path))
                })
                .collect::<Vec<_>>(),
            paths,
            cancel,
            walking
                .budget()
                .map(|budget| octocode_engine::scan_budget::walk_now() + budget),
        )?;
        if let Some(at) = cut {
            cut_walk_before(&mut native, at);
        }
        lines
    } else {
        vec![None; native.entries.len()]
    };
    let window = end_window(
        &mut native,
        window_end,
        q.scan_offset(),
        is_traversal(&sort),
    );
    resumes.keep(&mut native, walking);
    // `end_window` keeps a walk-order prefix: the counts of its entries.
    lines.truncate(native.entries.len());
    let mut rows = rows::rows_of(&native.entries, lines, root, security, true);
    rows::sort_rows(&mut rows, &sort);
    let mut walk = Walked::finish(native, rows, requested, q.scan_offset(), left, security);
    walk.meta.time_window = window;
    walk.meta.test_code_excluded = test_code.map(|left| left.through(walk.meta.walk_position));
    rows::group(&mut walk.rows, &sort);
    Ok(walk)
}

/// A `testCode` scope over a listing walk, and the walk positions of the
/// entries it left out: a window counts those it covers.
struct TestCodeLeft {
    scoped: std::sync::Arc<crate::tools::test_code::Scoped>,
    left: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
}

impl TestCodeLeft {
    fn of(scope: Option<octocode_contract_types::TestCode>, paths: &PathPolicy) -> Option<Self> {
        crate::tools::test_code::Scoped::of(scope, paths).map(|scoped| Self {
            scoped: std::sync::Arc::new(scoped),
            left: std::sync::Arc::default(),
        })
    }

    /// The engine's listing rule: the scope's verdict, recording each entry
    /// left out by position.
    fn rule(&self) -> octocode_engine::types::ListsEntry {
        let (scoped, left) = (
            std::sync::Arc::clone(&self.scoped),
            std::sync::Arc::clone(&self.left),
        );
        octocode_engine::types::ListsEntry(std::sync::Arc::new(move |path, is_dir, position| {
            let listed = scoped.lists(path, is_dir);
            if !listed {
                left.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(position);
            }
            listed
        }))
    }

    /// The entries left out at walk positions through `through`.
    fn through(self, through: usize) -> crate::tools::test_code::Excluded {
        let left = self
            .left
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::tools::test_code::Excluded {
            files: crate::num::u32_of_usize(
                left.iter()
                    .filter(|position| crate::num::usize_of(**position) <= through)
                    .count(),
            ),
            lines: 0,
        }
    }
}

/// End the walk before `native.entries[at]`, as its time budget does: the
/// entry and every one after it in walk order are uninspected, and
/// `next.continueWalk` resumes there.
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
