//! Listing rows, the one row shape `tree` and `files` share: how a walked
//! entry becomes a row, how rows sort, group and cost on a page, their line
//! counts and independently readable entry identities.
use super::cut::RowCost;
use octocode_runtime_base::response::result::ToolError;
use serde_json::{Value, json};

use octocode_contract_types::{StructureSearchSort, StructureSearchView};
use octocode_engine::types::{FileKind, FileSystemEntry};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::security::ContentSecurity;

mod line_counts;
mod window;

#[cfg(test)]
pub(super) use line_counts::line_count_reads;
pub(super) use line_counts::{Lines, count_lines};
pub(super) use window::{Ranking, digest_entry, rows_of};

/// What a listed entry is: anything that is neither a directory nor a
/// symlink lists as a file.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryKind {
    File,
    Directory,
    Symlink,
}

impl From<FileKind> for EntryKind {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::Directory => Self::Directory,
            FileKind::Symlink => Self::Symlink,
            FileKind::File | FileKind::Other => Self::File,
        }
    }
}

/// One listed entry.
pub(super) struct Row {
    /// Directory the entry is listed under, relative to the listed path
    /// (`""` is the path itself).
    pub(super) dir: String,
    /// The entry text inside its directory group ([`entry_text`]).
    pub(super) entry: String,
    /// The entry text of the names-only view: its name and kind, and under
    /// a size, line or time rank the one value it ranks by, so the order is
    /// readable without the details view.
    pub(super) ranked: String,
    /// The name the entry text starts with.
    pub(super) display_name: String,
    pub(super) kind: EntryKind,
    /// The entry relative to the listed path.
    pub(super) path: String,
    /// Redaction replaced part of [`Self::path`]: the row shows redacted text.
    pub(super) redacted: bool,
    /// The engine's name, which the name sort ranks by.
    pub(super) name: String,
    pub(super) size: i64,
    pub(super) modified: f64,
    /// The walked entry, for the continuation pages' change check.
    pub(super) source: std::path::PathBuf,
    /// The stamp of the walk's `lstat`; `None` when it read none (a view that
    /// shows only names).
    pub(super) stamp: Option<octocode_engine::stamp::SourceStamp>,
}

impl Row {
    /// What of the row a snapshot binds: what its page shows and cuts by.
    pub(super) fn shown(&self) -> impl serde::Serialize + '_ {
        (&self.dir, &self.entry, &self.ranked, &self.path)
    }

    /// The listed directory itself, which is the listing, never one of its
    /// rows.
    fn is_root_dir(entry: &FileSystemEntry, root: &std::path::Path) -> bool {
        entry.entry_type == FileKind::Directory
            && (entry.relative_path.is_empty() || std::path::Path::new(&entry.path) == root)
    }

    /// The path a walked entry is listed under, as delivered: relative to the
    /// walked root (`""` is the root itself, which a file root lists by its
    /// name), redacted, and whether redaction replaced any of it.
    fn listed_path(
        e: &FileSystemEntry,
        root: &std::path::Path,
        security: &ContentSecurity,
    ) -> (String, bool) {
        let is_root = e.relative_path.is_empty() || std::path::Path::new(&e.path) == root;
        let relative = if is_root {
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            e.relative_path.clone()
        };
        match security.sanitize_text(&relative, None).content {
            std::borrow::Cow::Borrowed(_) => (relative, false),
            std::borrow::Cow::Owned(text) => (text, true),
        }
    }

    /// The row of a walked entry listed under `path` ([`Self::listed_path`]),
    /// ranked by `key` (`None`: walk order).
    fn of(
        e: &FileSystemEntry,
        (path, redacted): (String, bool),
        lines: Option<Lines>,
        key: Option<octocode_contract_types::StructureSearchSortAnyOrderBy>,
    ) -> Self {
        use octocode_contract_types::StructureSearchSortAnyOrderBy as Key;
        let kind = EntryKind::from(e.entry_type);
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let dir = dir.to_owned();
        let modified_ms = e.modified_ms();
        let modified = modified_ms.unwrap_or(0.0);
        let size = content_size(e);
        let entry = entry_text(
            name,
            kind,
            size,
            // The line sort's own key, counted for every file it ranks,
            // is available to the details view without another read.
            lines,
            // Whole epoch milliseconds, shown as an ISO 8601 instant.
            modified_ms.map(|modified| octocode_runtime_base::num::i64_of_f64(modified.round())),
        );
        let ranked = entry_text(
            name,
            kind,
            size.filter(|_| key == Some(Key::Bytes)),
            lines.filter(|_| key == Some(Key::TotalLines)),
            modified_ms
                .filter(|_| key == Some(Key::ModifiedAt))
                .map(|modified| octocode_runtime_base::num::i64_of_f64(modified.round())),
        );
        Self {
            dir,
            entry,
            ranked,
            display_name: name.to_owned(),
            kind,
            path,
            redacted,
            name: e.name.clone(),
            size: size.unwrap_or(0),
            modified,
            source: std::path::PathBuf::from(&e.path),
            stamp: e.stamp,
        }
    }

    /// The entry text `view` shows before a page reads its details.
    fn shown_as(&self, view: StructureSearchView) -> &str {
        match view {
            StructureSearchView::Paths => &self.ranked,
            StructureSearchView::Details => &self.entry,
        }
    }
}

/// A non-directory entry's size. A directory's own length is its entry table,
/// not content: it is neither shown nor ranked (`sort:size` puts directories
/// last).
fn content_size(entry: &FileSystemEntry) -> Option<i64> {
    entry
        .size
        .filter(|_| EntryKind::from(entry.entry_type) != EntryKind::Directory)
}

/// Files, directories and file bytes of listing rows (`summary`): a
/// non-directory row is a file; bytes are regular files' (a symlink's own
/// length is no content).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) files: usize,
    pub(super) dirs: usize,
    pub(super) bytes: u64,
}

impl Tally {
    fn add(&mut self, kind: EntryKind, size: i64) {
        match kind {
            EntryKind::Directory => self.dirs += 1,
            EntryKind::Symlink => self.files += 1,
            EntryKind::File => {
                self.files += 1;
                self.bytes += u64::try_from(size).unwrap_or(0);
            }
        }
    }

    /// The tally of `rows`.
    pub(super) fn of_rows(rows: &[Row]) -> Self {
        let mut tally = Self::default();
        for row in rows {
            tally.add(row.kind, row.size);
        }
        tally
    }

    /// The tally of the walked `entries` at `indexes`, as their rows count.
    pub(super) fn of_entries(
        entries: &[FileSystemEntry],
        indexes: impl IntoIterator<Item = usize>,
    ) -> Self {
        let mut tally = Self::default();
        for entry in indexes.into_iter().filter_map(|at| entries.get(at)) {
            tally.add(
                EntryKind::from(entry.entry_type),
                content_size(entry).unwrap_or(0),
            );
        }
        tally
    }

    /// This tally and `other`'s together.
    pub(super) fn with(self, other: Self) -> Self {
        Self {
            files: self.files + other.files,
            dirs: self.dirs + other.dirs,
            bytes: self.bytes + other.bytes,
        }
    }
}

/// The rows of a page as its requested view shows them, each with its
/// directory: paths keeps names and kinds; with `detail`, file line counts
/// are read, for this page only.
pub(super) fn page_entries(
    rows: &[Row],
    view: StructureSearchView,
    detail: Option<&PathPolicy>,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<(String, String)>, ToolError> {
    if view == StructureSearchView::Paths {
        return Ok(rows
            .iter()
            .map(|row| (row.dir.clone(), row.ranked.clone()))
            .collect());
    }
    let Some(paths) = detail else {
        return Ok(rows
            .iter()
            .map(|row| (row.dir.clone(), row.entry.clone()))
            .collect());
    };
    let files = rows
        .iter()
        .map(|row| (row.kind != EntryKind::Directory).then_some(row.source.as_path()))
        .collect::<Vec<_>>();
    let (lines, _) = count_lines(&files, paths, cancel, None)?;
    Ok(rows
        .iter()
        .zip(lines)
        .map(|(row, lines)| {
            let entry = if row.kind == EntryKind::Directory {
                row.entry.clone()
            } else {
                entry_text(
                    &row.display_name,
                    row.kind,
                    Some(row.size),
                    lines.and_then(Lines::shown_in_details),
                    Some(octocode_runtime_base::num::i64_of_f64(row.modified.round())),
                )
            };
            (row.dir.clone(), entry)
        })
        .collect())
}

/// Group a window's rows (cut from `sort`ed rows) as the page lists them.
/// Walk order (`traversal`) groups by directory: the listed path's own
/// entries first, then each directory once, in walk order (the sort is
/// stable). Ranked sorts, `path` included (the default: a whole-scope rank by
/// path, where `memory.rs` < `memory/tests.rs` < `tests.rs` interleaves a
/// directory with its subdirectory), keep their global rank order exactly;
/// [`dir_groups`] then names each directory once for `path` ranks and
/// per run of adjacent rows for metric ranks.
///
/// Every listed directory keeps its own `name/` row in every view, beside
/// any `{dir, files}` group it heads: a group header is the parent of the
/// rows under it, which a filtered listing shows whether or not the
/// directory itself was selected, so only the row says it was.
pub(super) fn group(rows: &mut [Row], sort: &StructureSearchSort) {
    if super::files::is_traversal(sort) {
        rows.sort_by(|a, b| std::path::Path::new(&a.dir).cmp(std::path::Path::new(&b.dir)));
    }
}

/// The page's rows as the response lists them, `dir` workspace-relative
/// (the listed path's own entries included, `prefix` being its spelling), so
/// `dir + "/" + name` is a path a local tool takes. With `merge` (walk
/// order and the `path` sort: the order is only a way to name files),
/// one `{dir, files}` group per directory, in the order each first appears,
/// so a directory the rank returns to joins its group. Without it (a name, size,
/// lines or time rank, where the order is the answer) each run of adjacent
/// entries of one directory is a group and a directory the rank returns to
/// opens a new one. [`costs`] prices the same groups.
pub(super) fn dir_groups(rows: &[(String, String)], prefix: &str, merge: bool) -> Vec<Value> {
    use octocode_runtime_base::response::groups::{Entry, Order, groups};
    groups(
        rows.iter().map(|(dir, entry)| Entry {
            dir: super::group_dir(prefix, dir),
            item: json!(entry),
        }),
        if merge { Order::Merged } else { Order::Runs },
    )
}

/// A row costs its serialized entry, plus its group's header with the
/// directory rendered as the response shows it whenever it opens a group
/// (a page repeats the header of a group it continues). `deferred_detail`
/// reserves the detail fields a page adds. Costs are bound by the
/// snapshot, so every page of it cuts at the same rows.
pub(super) fn costs(
    rows: &[Row],
    prefix: &str,
    view: StructureSearchView,
    deferred_detail: bool,
) -> Vec<RowCost> {
    let dirs = rows
        .iter()
        .map(|row| super::group_dir(prefix, &row.dir))
        .collect::<Vec<_>>();
    octocode_runtime_base::response::groups::costs(dirs.iter().zip(rows).map(|(dir, row)| {
        let entry = octocode_engine::json_size::utf16_units(row.shown_as(view)).unwrap_or(0);
        // Reserve the deferred detail fields before paging so they cannot
        // overflow the window.
        (
            dir.as_str(),
            entry
                + usize::from(deferred_detail && row.kind != EntryKind::Directory)
                    * DETAIL_FIELDS_RESERVE,
        )
    }))
}

/// Characters the deferred detail fields add to one entry at most:
/// `, <usize>` lines (22) and `, <ISO 8601 instant>` (23 for a five-digit
/// year).
const DETAIL_FIELDS_RESERVE: usize = 45;

/// One listed entry inside its directory group: the name, `/` after a
/// directory, then ` (<fields>)` naming, in this order and as they apply,
/// the size in bytes (every non-directory entry), `symlink`, the line count
/// ([`Lines::field`]) and the modification time `<YYYY-MM-DDTHH:MM:SSZ>`
/// (UTC). Each field says what it is by its place and form: the bytes come
/// first, a line count is the integer after them (or `binary`,
/// `unreadable`), the time is the one ISO instant; a paths view's one sort
/// value is the field its `sort.by` names. The fields never contain ` (`, so
/// the last ` (` of an entry ending in `)` opens them, whatever the name
/// holds.
pub(super) fn entry_text(
    name: &str,
    kind: EntryKind,
    size: Option<i64>,
    line_count: Option<Lines>,
    modified_ms: Option<i64>,
) -> String {
    let mut fields = Vec::new();
    if let Some(size) = size {
        fields.push(size.to_string());
    }
    if kind == EntryKind::Symlink {
        fields.push("symlink".to_owned());
    }
    if let Some(lines) = line_count {
        fields.push(lines.field());
    }
    if let Some(modified) = modified_ms {
        // To the second, as every timestamp a tool states: a listing dates a
        // file, and `sort` ranks by the full time.
        fields.push(octocode_runtime_base::civil_date::iso8601_secs(
            modified.div_euclid(1_000),
        ));
    }
    octocode_tools_shared::root_read::entry_text(name, kind == EntryKind::Directory, &fields)
}
