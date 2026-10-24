//! Listing rows, the one row shape `tree` and `files` share: how a walked
//! entry becomes a row, how rows sort, group and cost on a page, their line
//! counts, and the read a listing's first page leads to.
use super::cut::RowCost;
use super::memo;
use crate::response::result::ToolError;
use serde_json::{Value, json};

use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use octocode_contract_types::StructureSearchQueryFilesSort;
use octocode_engine::types::{FileKind, FileSystemEntry};

mod line_counts;
mod read_lead;

use line_counts::line_count;
#[cfg(test)]
pub(super) use line_counts::line_count_reads;
pub(super) use line_counts::{Lines, count_lines};
pub(super) use read_lead::read_lead;

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
    /// Its line count, under `sort:lines` only.
    pub(super) lines: Option<Lines>,
    /// The walked entry, for the continuation pages' change check.
    pub(super) source: std::path::PathBuf,
    /// Its size and time as the walk read them (the stored walk's stamp).
    pub(super) seen: (u64, Option<std::time::SystemTime>),
}

impl Row {
    /// The row's size and time as the walk read them, for the continuation
    /// pages' change check.
    pub(super) fn seen(&self) -> memo::Seen<'_> {
        memo::Seen {
            path: &self.source,
            size: self.seen.0,
            modified: self.seen.1,
        }
    }

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

    /// The row of a walked entry. Rows name paths relative to the walked
    /// root: `""` is the root itself, which a file root lists by its name.
    /// `modified_shown` adds the modified time to the entry text.
    fn of(
        e: &FileSystemEntry,
        root: &std::path::Path,
        security: &ContentSecurity,
        modified_shown: bool,
        lines: Option<Lines>,
    ) -> Self {
        let is_root = e.relative_path.is_empty() || std::path::Path::new(&e.path) == root;
        let kind = EntryKind::from(e.entry_type);
        let relative = if is_root {
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            e.relative_path.clone()
        };
        let path = security.sanitize_text(&relative, None).content;
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let dir = dir.to_owned();
        let modified_ms = e.modified_ms();
        let modified = modified_ms.unwrap_or(0.0);
        // A directory's own length is its entry table, not content: it is
        // neither shown nor ranked (`sort:size` puts directories last).
        let size = e.size.filter(|_| kind != EntryKind::Directory);
        let entry = entry_text(
            name,
            kind,
            size,
            // The line sort's own key, counted for every file it ranks, shows
            // whatever the detail.
            lines,
            // Whole epoch milliseconds, shown as an ISO 8601 instant.
            modified_ms
                .filter(|_| modified_shown)
                .map(|modified| modified.round() as i64),
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
            lines,
            source: std::path::PathBuf::from(&e.path),
            seen: {
                let seen = memo::Seen::walked(std::path::Path::new(&e.path), e);
                (seen.size, seen.modified)
            },
        }
    }

    /// The row as its page shows it: with `detail`, a file's line count and
    /// modified time, read for this page's files only.
    pub(super) fn page_entry(
        &self,
        detail: Option<&PathPolicy>,
        cancel: &dyn CancellationCheck,
    ) -> Result<String, ToolError> {
        let Some(paths) = detail.filter(|_| self.kind != EntryKind::Directory) else {
            return Ok(self.entry.clone());
        };
        let lines = line_count(&self.source, paths, cancel)?;
        Ok(entry_text(
            &self.display_name,
            self.kind,
            Some(self.size),
            lines.shown_in_details(),
            Some(self.modified.round() as i64),
        ))
    }
}

/// The rows of a walk's entries, in walk order (the listed directory left
/// out), each with its line count from `lines` (the walk's entries, in the
/// same order). `modified_shown` adds the modified time to every entry.
pub(super) fn rows_of(
    entries: &[FileSystemEntry],
    lines: Vec<Option<Lines>>,
    root: &std::path::Path,
    security: &ContentSecurity,
    modified_shown: bool,
) -> Vec<Row> {
    entries
        .iter()
        .zip(lines)
        .filter(|(entry, _)| !Row::is_root_dir(entry, root))
        .map(|(entry, lines)| Row::of(entry, root, security, modified_shown, lines))
        .collect()
}

/// Rank rows by `sort`; path order is the walk order (the sort is stable).
pub(super) fn sort_rows(rows: &mut [Row], sort: &StructureSearchQueryFilesSort) {
    use octocode_contract_types::{SortOrder, StructureSearchQueryFilesSortVariant0By as Key};
    let StructureSearchQueryFilesSort::Variant0 { by, order } = sort else {
        return;
    };
    rows.sort_by(|a, b| {
        let compared = match by {
            Key::LineCount => Lines::rank(b.lines).cmp(&Lines::rank(a.lines)),
            Key::SizeBytes => a.size.cmp(&b.size),
            Key::Name => a.name.cmp(&b.name),
            Key::Path => a.path.cmp(&b.path),
            Key::ModifiedAt => a.modified.total_cmp(&b.modified),
        };
        if *order == SortOrder::Desc {
            compared.reverse()
        } else {
            compared
        }
    });
}

/// Group a window's rows (cut from `sort`ed rows) as the page lists them.
/// Path order groups by directory: the listed path's own entries first,
/// then each directory once, in walk order (the sort is stable). Other sorts
/// keep their rank order exactly; each page groups its own adjacent rows
/// ([`dir_groups`]).
pub(super) fn group(rows: &mut Vec<Row>, sort: &StructureSearchQueryFilesSort) {
    if super::files::is_traversal(sort) {
        rows.sort_by(|a, b| std::path::Path::new(&a.dir).cmp(std::path::Path::new(&b.dir)));
    }
    drop_grouped_dirs(rows);
}

/// Names each directory once: by its own `{dir, files}` group when the
/// group lists an entry, else by its bare `name/` row (a stub; one with
/// fields, `modified`, is no stub and keeps them). A stub is dropped only
/// when the directory's group survives, so a directory whose subdirectories
/// all have groups of their own keeps its stub instead of vanishing from
/// the listing, even when the two would land on different pages.
fn drop_grouped_dirs(rows: &mut Vec<Row>) {
    let stub = |row: &Row| row.entry.ends_with('/');
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
/// A path-ordered page holds each directory in one run, so it names each
/// once; a ranked page (`sort` by size, lines, name or time) keeps its rank
/// order exactly, and a directory it returns to opens a new group. Each
/// entry keeps the key it ranks by.
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
pub(super) fn costs(rows: &[Row], prefix: &str, deferred_detail: bool) -> Vec<RowCost> {
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
            RowCost {
                // Reserve the deferred detail fields before paging so
                // they cannot overflow the window.
                entry: octocode_engine::json_size::utf16_units(&row.entry).unwrap_or(0)
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
