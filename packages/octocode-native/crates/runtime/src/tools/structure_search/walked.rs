//! One listing walk's rows and what the walk saw beside them.
use super::cut::{Cut, TimeWindow, page_ranges};
use super::memo;
use super::probe::Uncovered;
use super::rows::{self, EntryKind, Row};
use super::walk_policy::WalkLeft;
use octocode_contract_types::StructureSearchView;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::mem::size_of;
use std::ops::Range;

/// One listing walk: its rows in listing order (cut to `limit`) and
/// what the walk saw beside them.
pub(super) struct Walked {
    pub(super) rows: Vec<Row>,
    pub(super) meta: WalkMeta,
    /// The digest of the rows a snapshot binds, computed once per walk: a
    /// stored walk's continuation pages reuse it instead of hashing every
    /// row again.
    pub(super) rows_digest: std::sync::OnceLock<String>,
    /// The page cut its first page made, with what it was cut for.
    page_cut: std::sync::OnceLock<(PageCut, Vec<Range<usize>>)>,
}

/// Everything beside its rows that a walk's page cut depends on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PageCut {
    /// The listed path as group headers spell it.
    pub(super) prefix: String,
    pub(super) view: StructureSearchView,
    /// Each file's entry reserves the detail fields a page adds.
    pub(super) deferred_detail: bool,
    /// Serialized chars a page holds.
    pub(super) budget: usize,
    /// Rows a page holds, when capped.
    pub(super) limit: Option<usize>,
}

/// What a listing walk saw beside its rows.
pub(super) struct WalkMeta {
    /// Listing rows before this window's end (`scanOffset` plus the rows
    /// this window took): where `next.nextPage` resumes.
    pub(super) covered: usize,
    pub(super) available: usize,
    pub(super) total_discovered: usize,
    pub(super) was_capped: bool,
    /// The walk ran out of time before this root-relative entry.
    pub(super) time_cut: Option<String>,
    pub(super) time_window: TimeWindow,
    /// Walk entries the window covers, `walkOffset` ones included.
    pub(super) walk_position: usize,
    /// Files, directories and file bytes of this window's rows, counted
    /// before directories with their own group fold into its header.
    pub(super) files: usize,
    pub(super) dirs: usize,
    /// `None` when the walk read no sizes (a view that shows only names).
    pub(super) bytes: Option<u64>,
    pub(super) uncovered: Uncovered,
    /// The directories the walk entered, which its stored rows depend on.
    pub(super) walked_dirs: Vec<octocode_engine::types::WalkedDir>,
    /// The walk read no row's metadata, so no row has a stamp to watch.
    pub(super) names_only: bool,
    /// Entries a `testCode` scope left out of the window (`None` unscoped).
    pub(super) test_code_excluded: Option<octocode_engine::types::TestCodeExcluded>,
}

impl Walked {
    /// What keeps this walk current. A row with no stamp in a walk that read
    /// metadata cannot be watched.
    pub(super) fn proof(
        &self,
        root: &std::path::Path,
        paths: &crate::policy::path::PathPolicy,
    ) -> Option<octocode_engine::stamp::Proof> {
        let listed = if self.meta.names_only {
            Vec::new()
        } else {
            self.rows
                .iter()
                .map(|row| Some((row.source.clone(), row.stamp?)))
                .collect::<Option<Vec<_>>>()?
        };
        memo::walk_proof(root, paths, &self.meta.walked_dirs, listed)
    }

    /// The row ranges of each page under `cut`. The cut of a walk's first
    /// page is kept for its later pages; any other is computed and not kept.
    pub(super) fn pages(&self, cut: PageCut) -> Cow<'_, [Range<usize>]> {
        let compute = |cut: &PageCut| {
            #[cfg(test)]
            COST_PASSES.with(|passes| passes.set(passes.get() + 1));
            page_ranges(
                &rows::costs(&self.rows, &cut.prefix, cut.view, cut.deferred_detail),
                cut.budget,
                cut.limit,
            )
        };
        let (kept, pages) = self.page_cut.get_or_init(|| {
            let pages = compute(&cut);
            (cut.clone(), pages)
        });
        if *kept == cut {
            Cow::Borrowed(pages)
        } else {
            Cow::Owned(compute(&cut))
        }
    }

    /// The heap this walk holds: the capacity of its row table and of every
    /// string in it, and what it keeps beside the rows.
    pub(super) fn weight(&self) -> usize {
        let meta = &self.meta;
        let row = |row: &Row| {
            row.dir.capacity()
                + row.entry.capacity()
                + row.display_name.capacity()
                + row.path.capacity()
                + row.name.capacity()
                + row.source.capacity()
        };
        let dir = |dir: &octocode_engine::types::WalkedDir| {
            dir.path.capacity()
                + dir.ignore_files.capacity() * size_of::<(std::path::PathBuf, bool)>()
                + dir
                    .ignore_files
                    .iter()
                    .map(|(file, _)| file.capacity())
                    .sum::<usize>()
        };
        let names = |names: &Vec<String>| {
            names.capacity() * size_of::<String>()
                + names.iter().map(String::capacity).sum::<usize>()
        };
        let cut = self.page_cut.get().map_or(0, |(cut, pages)| {
            cut.prefix.capacity() + pages.capacity() * size_of::<Range<usize>>()
        });
        size_of::<Self>()
            + self.rows.capacity() * size_of::<Row>()
            + self.rows.iter().map(row).sum::<usize>()
            + meta.walked_dirs.capacity() * size_of::<octocode_engine::types::WalkedDir>()
            + meta.walked_dirs.iter().map(dir).sum::<usize>()
            + names(&meta.uncovered.ignored)
            + names(&meta.uncovered.hidden)
            + names(&meta.uncovered.pruned)
            + meta.time_cut.as_ref().map_or(0, String::capacity)
            + self.rows_digest.get().map_or(0, String::capacity)
            + cut
    }
}

impl Walked {
    /// This window's `rows` of the `available` the walk ranked (listing order
    /// is stable, so the `scan_offset` before them are the ones earlier
    /// windows showed), with what the walk left out.
    pub(super) fn finish(
        native: octocode_engine::types::FileSystemQueryResult,
        rows: Vec<Row>,
        available: usize,
        scan_offset: usize,
        left: WalkLeft<'_>,
        security: &crate::security::ContentSecurity,
        names_only: bool,
    ) -> Self {
        let offset = scan_offset.min(available);
        // A non-directory row is a file; bytes are regular files' (a
        // symlink's own length is no content).
        let (mut files, mut dirs, mut bytes) = (0, 0, 0_u64);
        for row in &rows {
            match row.kind {
                EntryKind::Directory => dirs += 1,
                EntryKind::Symlink => files += 1,
                EntryKind::File => {
                    files += 1;
                    bytes += u64::try_from(row.size).unwrap_or(0);
                }
            }
        }
        let meta = WalkMeta {
            covered: offset + rows.len(),
            available,
            total_discovered: native.total_discovered as usize,
            was_capped: native.was_capped,
            time_cut: native
                .time_cut
                .map(|at| match security.sanitize_text(&at, None).content {
                    std::borrow::Cow::Borrowed(_) => at,
                    std::borrow::Cow::Owned(text) => text,
                }),
            time_window: TimeWindow::Whole,
            walk_position: native.walk_position as usize,
            files,
            dirs,
            bytes: (!names_only).then_some(bytes),
            walked_dirs: native.walked_dirs,
            names_only,
            uncovered: left.into_uncovered(
                native.ignored,
                native.walk_position,
                (native.skipped, native.permission_denied),
            ),
            test_code_excluded: None,
        };
        Self {
            rows,
            meta,
            rows_digest: std::sync::OnceLock::new(),
            page_cut: std::sync::OnceLock::new(),
        }
    }

    /// [`Self::rows_digest`]: the digest of each row's [`Sourced::shown`],
    /// in listing order.
    pub(super) fn rows_digest(&self) -> &str {
        self.rows_digest.get_or_init(|| {
            crate::digest::json_sha256(&self.rows.iter().map(Row::shown).collect::<Vec<_>>())
        })
    }

    /// Where this page cuts the listing. An early-exit walk that found one
    /// more match than the limit has more; only a full walk knows the total.
    pub(super) fn cut(
        &self,
        page: usize,
        pages: usize,
        requested: usize,
        early_exit: bool,
        (scan_offset, walk_offset): (usize, u32),
        snapshot: String,
    ) -> Cut {
        let meta = &self.meta;
        let limit_cut = if early_exit {
            meta.was_capped
        } else {
            meta.available > scan_offset.saturating_add(requested)
        };
        Cut {
            page,
            total_pages: pages.max(1),
            total: self.rows.len(),
            requested,
            available: meta.available,
            limit_cut,
            time_cut: meta.time_cut.clone(),
            time_window: meta.time_window,
            walk_position: meta.walk_position,
            walk_offset: walk_offset as usize,
            early_exit,
            total_discovered: meta.total_discovered,
            scan_offset,
            covered: meta.covered,
            snapshot,
        }
    }
}

#[cfg(test)]
thread_local! {
    static COST_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Times this thread priced a walk's rows.
#[cfg(test)]
pub(super) fn cost_passes() -> usize {
    COST_PASSES.with(std::cell::Cell::get)
}

/// Set a page's warnings: `cut`'s own (`warnings`). The query's notes
/// read the same for the whole result: they are notices, said once
/// ([`crate::response::paging::push_notice`]).
pub(super) fn push_warnings(out: &mut Value, query_notes: Vec<String>, cut: Vec<String>) {
    if !cut.is_empty() {
        out["warnings"] = json!(cut);
    }
    for note in query_notes {
        crate::response::paging::push_notice(out, note);
    }
}
