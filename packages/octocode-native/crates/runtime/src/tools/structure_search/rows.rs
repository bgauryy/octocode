//! Listing rows, the one row shape `tree` and `files` share: how a walked
//! entry becomes a row, how rows sort, group and cost on a page, their line
//! counts, and the read a listing's first page leads to.

use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::StructureSearchQueryFilesSort as Sort;
use octocode_engine::types::{FileKind, FileSystemEntry};

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
            // Whole epoch milliseconds: a float would print as `…737.0`.
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
pub(super) fn sort_rows(rows: &mut [Row], sort: Sort) {
    rows.sort_by(|a, b| match sort {
        Sort::Lines => Lines::rank(a.lines).cmp(&Lines::rank(b.lines)),
        Sort::Size => b.size.cmp(&a.size),
        Sort::Name => a.name.cmp(&b.name),
        // Component-wise, the walk order: `a/x` before `a-b/x`.
        Sort::Path => std::path::Path::new(&a.path).cmp(std::path::Path::new(&b.path)),
        Sort::Modified => b.modified.total_cmp(&a.modified),
    });
}

/// Group a window's rows (cut from `sort`ed rows) as the page lists them.
/// Path order groups by directory: the listed path's own entries first,
/// then each directory once, in walk order (the sort is stable). Other sorts
/// keep their order, so a directory may open several groups.
pub(super) fn group(rows: &mut Vec<Row>, sort: Sort) {
    if sort == Sort::Path {
        rows.sort_by(|a, b| std::path::Path::new(&a.dir).cmp(std::path::Path::new(&b.dir)));
    }
    drop_grouped_dirs(rows);
}

/// Names each directory once: by its own `{dir, files}` group when the
/// group lists an entry, else by its bare `name/` row (a stub; one with
/// fields, `modifiedMs`, is no stub and keeps them). A stub is dropped only
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

/// The page's rows as the response lists them: consecutive entries of one
/// directory become a `{dir, files}` group, `dir` workspace-relative (the
/// listed path's own entries included, `prefix` being its spelling), so
/// `dir + "/" + name` is a path a local tool takes.
pub(super) fn dir_groups(rows: &[(String, String)], prefix: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for (dir, entry) in rows {
        let dir = super::group_dir(prefix, dir);
        match out.last_mut() {
            Some(group) if group.get("dir").and_then(Value::as_str) == Some(dir.as_str()) => {
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

/// `next.read` on a listing's first page (`rows`): the best-ranked file
/// ([`read_rank`](crate::tools::root_read::read_rank), the ranking
/// ghStructure shares; any outline-able file ranks last), first among equal
/// ranks in listing order: of the small, hand-written files, else of the
/// files of at most [`READ_LEAD_FALLBACK_MAX_BYTES`]. Source and docs are
/// outlined (`minify:"symbols"`).
pub(super) fn read_lead(rows: &[Row], root: &std::path::Path) -> Option<Value> {
    let best = |preferred: bool| {
        rows.iter()
            .enumerate()
            .filter(|(_, row)| {
                row.kind == EntryKind::File
                    && if preferred {
                        row.size <= READ_LEAD_MAX_BYTES && !generated_or_fixture(&row.path)
                    } else {
                        row.size <= READ_LEAD_FALLBACK_MAX_BYTES
                    }
            })
            .filter_map(|(index, row)| {
                let depth = row.path.matches('/').count();
                let rank = crate::tools::root_read::read_rank(&row.path, depth, &[])
                    .or_else(|| crate::tools::outlines(&row.path).then_some(RANK_ANY))?;
                Some(((rank, depth, index), row))
            })
            .min_by_key(|(key, _)| *key)
            .map(|(_, row)| row)
    };
    let row = best(true).or_else(|| best(false))?;
    let path = if row.path.is_empty() || root.is_file() {
        root.to_path_buf()
    } else {
        root.join(&row.path)
    };
    let mut query = json!({"path": path.to_string_lossy()});
    if crate::tools::outlines(&row.path) {
        query["minify"] = json!("symbols");
    }
    Some(crate::response::result::Continuation::new(crate::id::ToolId::LocalFetch, query).build())
}

/// Rank of an outline-able file no [`read_rank`] tier names: it reads last.
///
/// [`read_rank`]: crate::tools::root_read::read_rank
const RANK_ANY: u8 = 6;

/// Largest file a listing's read lead points at by preference: a lead
/// over a multi-megabyte generated file would page for many calls.
pub(super) const READ_LEAD_MAX_BYTES: i64 = 64 * 1024;

/// Largest file a listing's read lead points at at all: past it, the
/// outline itself would page for many calls, so no lead is offered.
const READ_LEAD_FALLBACK_MAX_BYTES: i64 = 1024 * 1024;

/// A generated, minified, declaration, or lock file, or one under a test
/// or fixture directory: a poor first read of an unfamiliar tree.
pub(super) fn generated_or_fixture(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let generated = name.contains(".generated.")
        || name.contains("_generated")
        || name.contains(".min.")
        || name.ends_with(".d.ts")
        || name.ends_with(".lock");
    let fixture_dir = path.split('/').rev().skip(1).any(|dir| {
        matches!(
            dir,
            "test" | "tests" | "fixtures" | "__fixtures__" | "__snapshots__"
        )
    });
    generated || fixture_dir
}

/// Characters the deferred detail fields add to one entry at most:
/// `, totalLines=<usize>` and `, modifiedMs=<i64>`, 33 each.
const DETAIL_FIELDS_RESERVE: usize = 66;

/// One listed entry inside its directory group: the name, `/` after a
/// directory, then ` (<fields>)` naming the size in bytes (every non-directory
/// entry), `symlink`, the line count ([`Lines::field`]) and `modifiedMs=N` as
/// they apply. The fields never contain ` (`, so the last ` (` of an entry
/// ending in `)` opens them, whatever the name holds.
pub(super) fn entry_text(
    name: &str,
    kind: EntryKind,
    size: Option<i64>,
    line_count: Option<Lines>,
    modified_ms: Option<i64>,
) -> String {
    let mut text = name.to_owned();
    if kind == EntryKind::Directory {
        text.push('/');
    }
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
        fields.push(format!("modifiedMs={modified}"));
    }
    if !fields.is_empty() {
        text.push_str(&format!(" ({})", fields.join(", ")));
    }
    text
}

/// A file's line count as a listing shows it: counted, or why not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lines {
    Counted(usize),
    /// Binary content (the one binary definition: the leading-byte sniff, or
    /// a NUL anywhere) has no lines to count.
    Binary,
    /// The file could not be read (gone, denied, a read error): its count
    /// is unknown, not zero.
    Unreadable,
}

impl Lines {
    /// The entry field: `totalLines=N`, or the reason there is none.
    fn field(self) -> String {
        match self {
            Self::Counted(lines) => format!("totalLines={lines}"),
            Self::Binary => "binary".to_owned(),
            Self::Unreadable => "unreadable".to_owned(),
        }
    }

    /// The field a details listing shows: none for a file without lines.
    fn shown_in_details(self) -> Option<Self> {
        (self != Self::Counted(0)).then_some(self)
    }

    /// The line sort's key: counted files by count, most first, then the
    /// files without a count, then directories.
    fn rank(lines: Option<Self>) -> (u8, std::cmp::Reverse<usize>) {
        match lines {
            Some(Self::Counted(lines)) => (0, std::cmp::Reverse(lines)),
            Some(Self::Binary) => (1, std::cmp::Reverse(0)),
            Some(Self::Unreadable) => (2, std::cmp::Reverse(0)),
            None => (3, std::cmp::Reverse(0)),
        }
    }
}

/// Line-count workers opening files at once. Each validates, opens and
/// reads files, and kernel time grows with concurrent openers while wall
/// time levels off: over 2,280 files (warm cache, 12 cores, macOS) 4 workers
/// took 38-42 ms wall and 0.13 s sys, 12 workers 54-57 ms and 0.55-0.62 s.
const FILE_OPEN_WORKERS: usize = 4;

/// Workers for counting `entries` files ([`crate::tools::workers::workers`]),
/// at most [`FILE_OPEN_WORKERS`].
fn line_count_workers(entries: usize) -> usize {
    crate::tools::workers::workers(None, entries, Some(FILE_OPEN_WORKERS))
}

/// The line counts of `files` (`None`: a directory, never counted), on
/// [`line_count_workers`]: each worker takes the next file until none is
/// left or `deadline` passes (the first file is always counted, so a cut
/// walk still advances). With the deadline passed first, also the index of
/// the first file left uncounted: the counts are a prefix of `files` up to
/// it.
pub(super) fn count_lines(
    files: &[Option<&std::path::Path>],
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    deadline: Option<std::time::Instant>,
) -> Result<(Vec<Option<Lines>>, Option<usize>), ToolError> {
    let workers = line_count_workers(files.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let first_file = files.iter().position(Option::is_some);
    let counted = std::thread::scope(|scope| {
        let tasks = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut counted = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(file) = files.get(index) else {
                            return Ok(counted);
                        };
                        let Some(path) = file else { continue };
                        let late = Some(index) != first_file
                            && deadline
                                .is_some_and(|at| octocode_engine::scan_budget::walk_now() >= at);
                        let lines = if late {
                            None
                        } else {
                            Some(line_count(path, paths, cancel)?)
                        };
                        counted.push((index, lines));
                    }
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| {
                task.join().unwrap_or_else(|_| {
                    Err(ToolError::new(
                        ErrorCode::ExecutionFailed,
                        "A line-count worker failed.",
                    ))
                })
            })
            .collect::<Result<Vec<_>, ToolError>>()
    })?;
    let mut lines = vec![None; files.len()];
    let mut cut = None::<usize>;
    for (index, count) in counted.into_iter().flatten() {
        match count {
            Some(count) => lines[index] = Some(count),
            None => cut = Some(cut.map_or(index, |at| at.min(index))),
        }
    }
    if let Some(at) = cut {
        lines.truncate(at);
    }
    Ok((lines, cut))
}

/// The lines of the file at `path`, revalidated against the path policy
/// right before it is read.
pub(super) fn line_count(
    path: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Lines, ToolError> {
    use std::io::Read;
    #[cfg(test)]
    LINE_COUNT_READS.with(|reads| reads.set(reads.get() + 1));
    cancel.check().map_err(ToolError::cancelled)?;
    // Revalidate immediately before reading: discovery authorization is not
    // permission to follow a subsequently changed link or open a special file.
    let Ok(validated) = paths.validate_read(path) else {
        return Ok(Lines::Unreadable);
    };
    let Ok(mut file) = std::fs::File::open(validated.canonical) else {
        return Ok(Lines::Unreadable);
    };
    let mut buffer = [0_u8; 16 * 1024];
    // Newlines terminate lines; a non-empty final line without one still counts.
    let mut lines = 0_usize;
    let mut last_byte = None;
    loop {
        cancel.check().map_err(ToolError::cancelled)?;
        match file.read(&mut buffer) {
            Ok(0) => {
                return Ok(Lines::Counted(
                    lines + usize::from(last_byte.is_some_and(|b| b != b'\n')),
                ));
            }
            Ok(bytes) => {
                let read = &buffer[..bytes];
                if (last_byte.is_none()
                    && octocode_engine::text::sniff(read)
                        == octocode_engine::text::BinaryVerdict::Binary)
                    || read.contains(&0)
                {
                    return Ok(Lines::Binary);
                }
                lines += read.iter().filter(|byte| **byte == b'\n').count();
                last_byte = read.last().copied();
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Ok(Lines::Unreadable),
        }
    }
}

#[cfg(test)]
thread_local! {
    static LINE_COUNT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn line_count_reads() -> usize {
    LINE_COUNT_READS.with(std::cell::Cell::get)
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
