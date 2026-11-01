//! One listing walk's rows and what the walk saw beside them.
use super::cut::{Cut, TimeWindow};
use super::memo;
use super::probe::Uncovered;
use super::rows::{EntryKind, Row};
use super::walk_policy::WalkLeft;
use serde_json::{Value, json};

/// One listing walk: its rows in listing order (cut to `limit`) and
/// what the walk saw beside them.
pub(super) struct Walked {
    pub(super) rows: Vec<Row>,
    pub(super) meta: WalkMeta,
    /// The digest of the rows a snapshot binds, computed once per walk: a
    /// stored walk's continuation pages reuse it instead of hashing every
    /// row again.
    pub(super) rows_digest: std::sync::OnceLock<String>,
}

/// What a listing walk saw beside its rows.
pub(super) struct WalkMeta {
    /// Listing rows before this window's end (`scanOffset` plus the rows
    /// this window took): where `next.continueWalk` resumes.
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
    pub(super) bytes: u64,
    pub(super) uncovered: Uncovered,
    /// Entries a `testCode` scope left out of the window (`None` unscoped).
    pub(super) test_code_excluded: Option<crate::tools::test_code::Excluded>,
}

impl memo::Listed for Walked {
    fn sources(&self) -> Vec<memo::Seen<'_>> {
        self.rows.iter().map(Row::seen).collect()
    }
}

impl Walked {
    /// The walk's `rows` (listing order) past the `scan_offset` rows earlier
    /// windows listed (listing order is stable, so those are the ones they
    /// showed), cut to this window's `requested`, with what the walk left
    /// out.
    pub(super) fn finish(
        native: octocode_engine::types::FileSystemQueryResult,
        mut rows: Vec<Row>,
        requested: usize,
        scan_offset: usize,
        left: WalkLeft<'_>,
        security: &crate::security::ContentSecurity,
    ) -> Self {
        let available = rows.len();
        rows.truncate(scan_offset.saturating_add(requested));
        let offset = scan_offset.min(rows.len());
        rows.drain(..offset);
        // A non-directory row is a file; bytes are regular files' (a
        // symlink's own length is no content).
        let (mut files, mut dirs, mut bytes) = (0, 0, 0);
        for row in &rows {
            match row.kind {
                EntryKind::Directory => dirs += 1,
                EntryKind::Symlink => files += 1,
                EntryKind::File => {
                    files += 1;
                    bytes += row.seen.0;
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
                .map(|at| security.sanitize_text(&at, None).content),
            time_window: TimeWindow::Whole,
            walk_position: native.walk_position as usize,
            files,
            dirs,
            bytes,
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
