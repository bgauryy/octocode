//! One listing walk's rows and what the walk saw beside them.
use super::cut::{Cut, TimeWindow, page_ranges};
use super::memo;
use super::probe::Uncovered;
use super::rows::{self, Row, Tally};
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
    /// The proof of the directories and rule files of a listing kept whole
    /// ([`super::windows`]), which its pages share instead of `walked_dirs`.
    pub(super) dirs: Option<octocode_engine::stamp::Proof>,
}

/// Everything beside its rows that a walk's page cut depends on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PageCut {
    /// The listed path as group headers spell it.
    pub(super) prefix: String,
    pub(super) view: StructureSearchView,
    /// Each file's entry reserves the detail fields a page adds.
    pub(super) deferred_detail: bool,
    /// The page lists its entries flat ([`rows::lists_flat`]): the listed
    /// path as their row paths spell it ([`super::flat_prefix`]).
    pub(super) flat: Option<String>,
    /// Serialized chars a page holds.
    pub(super) budget: usize,
    /// Rows a page holds, when capped.
    pub(super) limit: Option<usize>,
}

/// What proves a listing beyond its rows and directories: what a `testCode` scope,
/// a metadata filter or a replay read, and what the windows before a kept cursor read.
#[derive(Clone, Default)]
pub(super) enum Verdicts {
    #[default]
    Unscoped,
    Proved(octocode_engine::stamp::Proof),
    /// No proof could be taken, so the rows are not kept.
    Unproved,
}

impl Verdicts {
    /// The verdicts of a walk: what a `testCode` scope read, the entries it
    /// `evaluated`, and what the windows `earlier` read. `watched` is false when
    /// something it read has no proof: a scope's read, or a filter no stamp shows
    /// the change of.
    pub(super) fn of(
        scoped: Option<octocode_engine::stamp::Proof>,
        evaluated: Vec<octocode_engine::types::EvaluatedEntry>,
        earlier: Option<&octocode_engine::stamp::Proof>,
        watched: bool,
    ) -> Self {
        if !watched {
            return Self::Unproved;
        }
        let Some(taken) = evaluated
            .into_iter()
            .map(|entry| Some((entry.path, entry.stamp?)))
            .collect::<Option<Vec<_>>>()
        else {
            return Self::Unproved;
        };
        if scoped.is_none() && taken.is_empty() && earlier.is_none() {
            return Self::Unscoped;
        }
        let proof = scoped
            .unwrap_or_default()
            .with_taken(octocode_engine::stamp::Link::Itself, taken);
        Self::Proved(match earlier {
            Some(earlier) => proof.with(earlier),
            None => proof,
        })
    }

    /// The bytes its proof holds.
    fn weight(&self) -> usize {
        match self {
            Self::Proved(proof) => proof.weight(),
            Self::Unscoped | Self::Unproved => 0,
        }
    }
}

/// What a listing walk saw beside its rows.
#[derive(Clone)]
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
    pub(super) window: Tally,
    /// The same of every row a walk that ranked its whole scope ranked, the
    /// windows' before and after this one included: what a `complete`
    /// count totals. `None` for a walk-order or paused walk.
    pub(super) ranked: Option<Tally>,
    pub(super) uncovered: Uncovered,
    /// The directories the walk entered, which its stored rows depend on.
    pub(super) walked_dirs: Vec<octocode_engine::types::WalkedDir>,
    /// The walk read no row's metadata, so no row has a stamp to watch.
    pub(super) names_only: bool,
    /// What a `testCode` scope left of the window (`None` unscoped).
    pub(super) test_code: Option<super::walk_policy::TestCodeWindow>,
    pub(super) verdicts: Verdicts,
    /// What the rows before this window hash to, when the query pinned them
    /// ([`super::windows`]).
    pub(super) pinned_before: Option<String>,
    /// The listing through this window when it runs past one
    /// ([`super::windows`]): what its pages and next window pin.
    pub(super) pins_through: Option<String>,
}

impl Walked {
    /// What keeps this walk current. A row with no stamp in a walk that read
    /// metadata cannot be watched.
    pub(super) fn proof(
        &self,
        root: &std::path::Path,
        paths: &octocode_runtime_base::policy::path::PathPolicy,
    ) -> Option<octocode_engine::stamp::Proof> {
        let listed = memo::row_stamps(
            self.meta.names_only,
            self.rows
                .iter()
                .map(|row| (row.source.as_path(), row.stamp)),
        )?;
        if let Some(dirs) = &self.dirs {
            return Some(if listed.is_empty() {
                dirs.clone()
            } else {
                dirs.clone()
                    .with_taken(octocode_engine::stamp::Link::Itself, listed)
            });
        }
        let walked = memo::walk_proof(root, paths, &self.meta.walked_dirs, listed)?;
        match &self.meta.verdicts {
            Verdicts::Unscoped => Some(walked),
            Verdicts::Proved(verdicts) => Some(walked.with(verdicts)),
            Verdicts::Unproved => None,
        }
    }

    /// Whether [`Self::proof`] is the listing's own, which it already weighs.
    pub(super) fn shares_proof(&self) -> bool {
        self.dirs.is_some() && self.rows.iter().all(|row| row.stamp.is_none())
    }

    /// The row ranges of each page under `cut`. The cut of a walk's first
    /// page is kept for its later pages; any other is computed and not kept.
    pub(super) fn pages(&self, cut: PageCut) -> Cow<'_, [Range<usize>]> {
        let compute = |cut: &PageCut| {
            #[cfg(test)]
            COST_PASSES.with(|passes| passes.set(passes.get() + 1));
            page_ranges(
                &rows::costs(
                    &self.rows,
                    &cut.prefix,
                    cut.view,
                    cut.deferred_detail,
                    cut.flat.as_deref(),
                ),
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
        let row = |row: &Row| {
            row.dir.capacity()
                + row.entry.capacity()
                + row.display_name.capacity()
                + row.path.capacity()
                + row.name.capacity()
                + row.source.capacity()
        };
        let cut = self.page_cut.get().map_or(0, |(cut, pages)| {
            cut.prefix.capacity() + pages.capacity() * size_of::<Range<usize>>()
        });
        size_of::<Self>()
            + self.rows.capacity() * size_of::<Row>()
            + self.rows.iter().map(row).sum::<usize>()
            + self.meta.weight()
            + self.rows_digest.get().map_or(0, String::capacity)
            + cut
    }
}

impl WalkMeta {
    /// The heap this holds.
    pub(super) fn weight(&self) -> usize {
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
        self.walked_dirs.capacity() * size_of::<octocode_engine::types::WalkedDir>()
            + self.walked_dirs.iter().map(dir).sum::<usize>()
            + names(&self.uncovered.ignored)
            + names(&self.uncovered.hidden)
            + names(&self.uncovered.pruned)
            + self.time_cut.as_ref().map_or(0, String::capacity)
            + self.verdicts.weight()
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
        security: &octocode_runtime_base::security::ContentSecurity,
        names_only: bool,
    ) -> Self {
        let offset = scan_offset.min(available);
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
            window: Tally::of_rows(&rows),
            ranked: None,
            walked_dirs: native.walked_dirs,
            names_only,
            uncovered: left.into_uncovered(
                native.ignored,
                native.walk_position,
                (native.skipped, native.permission_denied),
            ),
            test_code: None,
            verdicts: Verdicts::Unscoped,
            pinned_before: None,
            pins_through: None,
        };
        Self::of(rows, meta, None)
    }

    /// This window's `rows`, cut from a listing kept whole with `listing` beside
    /// its rows, proved by its directories (`dirs`) and these rows.
    pub(super) fn served(
        rows: Vec<Row>,
        listing: &WalkMeta,
        scan_offset: usize,
        dirs: octocode_engine::stamp::Proof,
    ) -> Self {
        let meta = WalkMeta {
            covered: scan_offset.min(listing.available) + rows.len(),
            window: Tally::of_rows(&rows),
            ..listing.clone()
        };
        Self::of(rows, meta, Some(dirs))
    }

    fn of(rows: Vec<Row>, meta: WalkMeta, dirs: Option<octocode_engine::stamp::Proof>) -> Self {
        Self {
            rows,
            meta,
            rows_digest: std::sync::OnceLock::new(),
            page_cut: std::sync::OnceLock::new(),
            dirs,
        }
    }

    /// [`Self::rows_digest`]: the digest of each row's [`Sourced::shown`](Row::shown),
    /// in listing order.
    pub(super) fn rows_digest(&self) -> &str {
        self.rows_digest.get_or_init(|| {
            octocode_runtime_base::digest::json_sha256(
                &self.rows.iter().map(Row::shown).collect::<Vec<_>>(),
            )
        })
    }

    /// Where this page cuts the listing. An early-exit walk that found one
    /// more match than the limit has more; only a full walk knows the total.
    /// `prefix` is the listed path as the response spells it.
    pub(super) fn cut(
        &self,
        page: usize,
        pages: usize,
        requested: usize,
        early_exit: bool,
        (scan_offset, walk_offset): (usize, u32),
        snapshot: String,
        prefix: &str,
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
            // Named as a row of the listing names its path: from the
            // workspace root, not from the listed directory.
            time_cut: meta
                .time_cut
                .as_deref()
                .map(|at| super::group_dir(prefix, at)),
            time_window: meta.time_window,
            walk_position: meta.walk_position,
            walk_offset: walk_offset as usize,
            early_exit,
            total_discovered: meta.total_discovered,
            scan_offset,
            covered: meta.covered,
            snapshot,
            pins_through: meta.pins_through.clone(),
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
/// ([`octocode_runtime_base::response::paging::push_notice`]).
pub(super) fn push_warnings(out: &mut Value, query_notes: Vec<String>, cut: Vec<String>) {
    if !cut.is_empty() {
        out["warnings"] = json!(cut);
    }
    for note in query_notes {
        octocode_runtime_base::response::paging::push_notice(out, note);
    }
}
