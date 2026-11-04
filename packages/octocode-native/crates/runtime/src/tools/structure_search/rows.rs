//! Listing rows, the one row shape `tree` and `files` share: how a walked
//! entry becomes a row, how rows sort, group and cost on a page, their line
//! counts, and the read a listing's first page leads to.
use super::cut::RowCost;
use crate::response::result::ToolError;
use serde_json::{Value, json};

use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use octocode_contract_types::{StructureSearchSort, StructureSearchView};
use octocode_engine::types::{FileKind, FileSystemEntry};

mod line_counts;
mod read_lead;
mod window;

#[cfg(test)]
pub(super) use line_counts::line_count_reads;
pub(super) use line_counts::{Lines, count_lines};
pub(super) use read_lead::read_lead;
pub(super) use window::window_rows;

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
    /// The name the entry text starts with.
    pub(super) display_name: String,
    pub(super) kind: EntryKind,
    /// The entry relative to the listed path.
    pub(super) path: String,
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
        (&self.dir, &self.entry, &self.path)
    }

    /// The listed directory itself, which is the listing, never one of its
    /// rows.
    fn is_root_dir(entry: &FileSystemEntry, root: &std::path::Path) -> bool {
        entry.entry_type == FileKind::Directory
            && (entry.relative_path.is_empty() || std::path::Path::new(&entry.path) == root)
    }

    /// The path a walked entry is listed under, as delivered: relative to the
    /// walked root (`""` is the root itself, which a file root lists by its
    /// name), redacted.
    fn listed_path(
        e: &FileSystemEntry,
        root: &std::path::Path,
        security: &ContentSecurity,
    ) -> String {
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
            std::borrow::Cow::Borrowed(_) => relative,
            std::borrow::Cow::Owned(text) => text,
        }
    }

    /// The row of a walked entry listed under `path` ([`Self::listed_path`]).
    fn of(e: &FileSystemEntry, path: String, lines: Option<Lines>) -> Self {
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
            modified_ms.map(|modified| crate::num::i64_of_f64(modified.round())),
        );
        Self {
            dir,
            entry,
            display_name: name.to_owned(),
            kind,
            path,
            name: e.name.clone(),
            size: size.unwrap_or(0),
            modified,
            source: std::path::PathBuf::from(&e.path),
            stamp: e.stamp,
        }
    }

    fn path_entry(&self) -> String {
        entry_text(&self.display_name, self.kind, None, None, None)
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
            .map(|row| (row.dir.clone(), row.path_entry()))
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
                    Some(crate::num::i64_of_f64(row.modified.round())),
                )
            };
            (row.dir.clone(), entry)
        })
        .collect())
}

/// Group a window's rows (cut from `sort`ed rows) as the page lists them.
/// Path order groups by directory: the listed path's own entries first,
/// then each directory once, in walk order (the sort is stable). Other sorts
/// keep their global rank order exactly; each page groups only its runs of
/// adjacent rows ([`dir_groups`]).
pub(super) fn group(rows: &mut Vec<Row>, sort: &StructureSearchSort, view: StructureSearchView) {
    if super::files::is_traversal(sort) {
        rows.sort_by(|a, b| std::path::Path::new(&a.dir).cmp(std::path::Path::new(&b.dir)));
    }
    drop_grouped_dirs(rows, view);
}

/// Names each directory once: by its own `{dir, files}` group when the
/// group lists an entry, else by its bare `name/` row (a stub: every directory
/// row of a view that shows no fields; one with fields, `modified`, is no stub
/// and keeps them). A stub is dropped only
/// when the directory's group survives, so a directory whose subdirectories
/// all have groups of their own keeps its stub instead of vanishing from
/// the listing, even when the two would land on different pages.
fn drop_grouped_dirs(rows: &mut Vec<Row>, view: StructureSearchView) {
    let stub = |row: &Row| {
        row.kind == EntryKind::Directory
            && (view == StructureSearchView::Paths || row.entry.ends_with('/'))
    };
    let mut grouped = rows
        .iter()
        .filter(|row| !stub(row))
        .map(|row| row.dir.clone())
        .collect::<std::collections::HashSet<_>>();
    // A group's rows sit deeper than the stub naming it, so the deepest
    // groups settle first and decide the stubs above them.
    let depth = |dir: &str| {
        if dir.is_empty() {
            0
        } else {
            dir.matches('/').count() + 1
        }
    };
    let mut stubs = (0..rows.len())
        .filter(|&index| stub(&rows[index]))
        .collect::<Vec<_>>();
    stubs.sort_by_key(|&index| std::cmp::Reverse(depth(&rows[index].dir)));
    let mut dropped = std::collections::HashSet::new();
    for index in stubs {
        if grouped.contains(&rows[index].path) {
            dropped.insert(index);
        } else {
            grouped.insert(rows[index].dir.clone());
        }
    }
    let mut index = 0;
    rows.retain(|_| {
        index += 1;
        !dropped.contains(&(index - 1))
    });
}

/// The page's rows as the response lists them, in page order: each run of
/// adjacent entries of one directory becomes one `{dir, files}` group, `dir`
/// workspace-relative (the listed path's own entries included, `prefix`
/// being its spelling), so `dir + "/" + name` is a path a local tool takes.
/// A path-ordered page holds each directory in one run ([`group`]), so it
/// names each once; a ranked page (`sort` by size, lines, name or time)
/// keeps its global rank order exactly, and a directory it returns to opens
/// a new group. [`costs`] prices the same runs.
pub(super) fn dir_groups(rows: &[(String, String)], prefix: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for (dir, entry) in rows {
        let dir = super::group_dir(prefix, dir);
        match out.last_mut() {
            Some(group) if group["dir"] == dir.as_str() => {
                if let Some(items) = group["files"].as_array_mut() {
                    items.push(json!(entry));
                }
            }
            _ => out.push(json!({"dir": dir, "files": [entry]})),
        }
    }
    out
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
    let mut headers = std::collections::HashMap::<&str, usize>::new();
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let header = *headers.entry(row.dir.as_str()).or_insert_with(|| {
                octocode_engine::json_size::utf16_units(
                    &json!({"dir":super::group_dir(prefix, &row.dir),"files":[]}),
                )
                .unwrap_or(0)
                    + 1
            });
            let entry = match view {
                StructureSearchView::Paths => {
                    octocode_engine::json_size::utf16_units(&row.path_entry())
                }
                StructureSearchView::Details => octocode_engine::json_size::utf16_units(&row.entry),
            }
            .unwrap_or(0);
            RowCost {
                // Reserve the deferred detail fields before paging so
                // they cannot overflow the window.
                entry: entry
                    + 1
                    + usize::from(deferred_detail && row.kind != EntryKind::Directory)
                        * DETAIL_FIELDS_RESERVE,
                header,
                continues: index > 0 && rows[index - 1].dir == row.dir,
            }
        })
        .collect()
}

/// Characters the deferred detail fields add to one entry at most:
/// `, totalLines=<usize>` (33) and `, modified=<ISO 8601 instant>` (35 for
/// a five-digit year).
const DETAIL_FIELDS_RESERVE: usize = 68;

/// One listed entry inside its directory group: the name, `/` after a
/// directory, then ` (<fields>)` naming the size in bytes (every non-directory
/// entry), `symlink`, the line count ([`Lines::field`]) and
/// `modified=<YYYY-MM-DDTHH:MM:SS.mmmZ>` (UTC) as they apply. The fields never contain ` (`, so the last ` (` of an entry
/// ending in `)` opens them, whatever the name holds.
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
        fields.push(format!(
            "modified={}",
            crate::civil_date::iso8601_millis(modified)
        ));
    }
    crate::tools::root_read::entry_text(name, kind == EntryKind::Directory, &fields)
}
