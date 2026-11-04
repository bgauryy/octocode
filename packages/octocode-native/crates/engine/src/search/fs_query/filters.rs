//! What one entry is listed as: its kind, the name, metadata, time and
//! permission filters it passes, and the row it becomes.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use ignore::gitignore::Gitignore;
use regex::Regex;

use super::compiled::CompiledQuery;
use super::walker::{QueryState, entered_empty};
use crate::error::{Error, ErrorKind};
use crate::search::glob;
use crate::text::extension_of;
use crate::types::{EntryType, FileKind, FileSystemEntry};

/// Whether the `entry_type` filter lists an entry of `kind`.
fn lists(entry_type: EntryType, kind: EntryKind) -> bool {
    match entry_type {
        EntryType::File => kind.is_file,
        EntryType::Directory => kind.is_dir,
    }
}

/// An entry's type, from its directory listing or its `symlink_metadata`.
#[derive(Clone, Copy)]
pub(super) struct EntryKind {
    pub(super) is_dir: bool,
    pub(super) is_file: bool,
    pub(super) is_symlink: bool,
}

impl EntryKind {
    pub(super) fn of(file_type: fs::FileType) -> Self {
        Self {
            is_dir: file_type.is_dir(),
            is_file: file_type.is_file(),
            is_symlink: file_type.is_symlink(),
        }
    }
}

impl From<fs::FileType> for EntryKind {
    fn from(file_type: fs::FileType) -> Self {
        Self::of(file_type)
    }
}

#[cfg(test)]
thread_local! {
    /// Entry `lstat`s this thread's walks made.
    pub(super) static ENTRY_STATS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Directories this thread's walks read only to tell they are empty.
    pub(super) static EMPTINESS_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `symlink_metadata`, counting a failure as a skipped entry.
pub(super) fn lstat(path: &Path, state: &mut QueryState) -> Option<fs::Metadata> {
    #[cfg(test)]
    ENTRY_STATS.with(|stats| stats.set(stats.get() + 1));
    match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(err) => {
            state.unreadable(&err);
            None
        }
    }
}

/// Count and record `path` when it matches. Name and type filters run
/// first; the entry is stat'ed (into `metadata`) only when they pass and
/// something reads it ([`CompiledQuery::stats_entries`]). `Err` when that stat
/// failed: the entry is skipped like an unreadable one.
pub(super) fn visit_path(
    path: &Path,
    name: &str,
    depth: u32,
    kind: EntryKind,
    metadata: &mut Option<fs::Metadata>,
    query: &CompiledQuery,
    state: &mut QueryState,
) -> Result<(), ()> {
    if depth < query.min_depth {
        return Ok(());
    }
    if query.max_depth.is_some_and(|max_depth| depth > max_depth) {
        return Ok(());
    }
    if !matches_names(path, name, kind, query) {
        return Ok(());
    }
    if query.stats_entries && metadata.is_none() {
        *metadata = Some(lstat(path, state).ok_or(())?);
    }
    if metadata
        .as_ref()
        .is_some_and(|metadata| !matches_metadata(path, metadata, (depth, kind), query, state))
    {
        return Ok(());
    }
    if query
        .lists
        .as_ref()
        .is_some_and(|lists| !(lists.0)(path, kind.is_dir, state.position))
    {
        return Ok(());
    }

    state.total_discovered += 1;
    if state.entries.len() >= query.limit {
        if state.first_unlisted.is_none() {
            state.first_unlisted = Some(path.to_path_buf());
        }
        return Ok(());
    }

    state.entries.push(to_entry(
        path,
        name,
        state.position,
        kind,
        metadata.as_ref(),
        query,
    ));
    Ok(())
}

/// The filters an entry's name, path and type decide.
fn matches_names(path: &Path, name: &str, kind: EntryKind, query: &CompiledQuery) -> bool {
    query
        .name_filter
        .lists(path, name, kind.is_dir, kind.is_file)
        && query
            .entry_type
            .is_none_or(|entry_type| lists(entry_type, kind))
}

/// A query's name filters, compiled once: `names` globs (see [`glob`]: an
/// entry is listed when one matches it), `extensions` (OR-combined, files
/// only) and a `regex` over the entry's name. An empty filter lists all.
pub struct NameFilter {
    names: Option<Gitignore>,
    extensions: Vec<String>,
    regex: Option<Regex>,
}

impl NameFilter {
    /// The filters of a walk rooted at `root`. A glob or regex that does not
    /// compile fails ([`ErrorKind::InvalidGlob`], [`ErrorKind::InvalidPattern`]):
    /// a filter is never silently dropped.
    pub fn new(
        root: &Path,
        names: &[String],
        extensions: &[String],
        regex: Option<&str>,
    ) -> Result<Self, Error> {
        // An extension never holds a glob or path character: `fs_*` or
        // `src/*.ts` is a name glob, which would match nothing here.
        if let Some(glob) = extensions
            .iter()
            .find(|value| value.contains(['*', '?', '[', '/', '\\']))
        {
            return Err(Error::with_kind(
                ErrorKind::InvalidPattern,
                format!(
                    "extensions take bare extensions such as \"rs\"; \"{glob}\" is a name glob: pass it as include"
                ),
            ));
        }
        Ok(Self {
            names: glob::set(root, names.iter().map(String::as_str), "names")?,
            extensions: normalize_extensions(extensions.to_vec()),
            regex: regex
                .map(|pattern| {
                    Regex::new(pattern).map_err(|err| {
                        Error::with_kind(
                            ErrorKind::InvalidPattern,
                            format!("Invalid regex for local filesystem query: {err}"),
                        )
                    })
                })
                .transpose()?,
        })
    }

    /// Whether the entry at `path` named `name` passes every filter. A
    /// directory never passes an extension filter (the walk still enters
    /// it).
    pub fn lists(&self, path: &Path, name: &str, is_dir: bool, is_file: bool) -> bool {
        self.names
            .as_ref()
            .is_none_or(|names| glob::matches(names, path, is_dir))
            && self.regex.as_ref().is_none_or(|regex| regex.is_match(name))
            && (self.extensions.is_empty() || (is_file && has_extension(name, &self.extensions)))
    }
}

/// The filters that need the entry's metadata.
fn matches_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    place: (u32, EntryKind),
    query: &CompiledQuery,
    state: &mut QueryState,
) -> bool {
    if query.empty && !is_empty(path, metadata, place, query, state) {
        return false;
    }
    // A size filter is about content: a directory's own length (its entry
    // table) and a link's (its target's spelling) are none, so neither ever
    // matches one.
    let sized = query.size_min.is_some() || query.size_max.is_some();
    if sized && (metadata.is_dir() || metadata.file_type().is_symlink()) {
        return false;
    }
    if let Some(min_size) = query.size_min
        && metadata.len() < min_size
    {
        return false;
    }
    if let Some(max_size) = query.size_max
        && metadata.len() > max_size
    {
        return false;
    }
    if !matches_time_filters(metadata, query) {
        return false;
    }
    matches_permissions(path, metadata, query)
}

/// Whether `name` ends in one of `extensions`, normalized as a query's are
/// (lowercase, no leading dot).
pub(crate) fn has_extension(name: &str, extensions: &[String]) -> bool {
    let extension = extension_of(name, true, "");
    !extension.is_empty() && extensions.iter().any(|allowed| allowed == &extension)
}

fn matches_time_filters(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    within_time_bounds(
        metadata.modified().ok(),
        query.modified_from,
        query.modified_to,
    ) && within_time_bounds(
        metadata.accessed().ok(),
        query.accessed_from,
        query.accessed_to,
    )
}

fn within_time_bounds(
    time: Option<SystemTime>,
    from: Option<SystemTime>,
    to: Option<SystemTime>,
) -> bool {
    if from.is_none() && to.is_none() {
        return true;
    }
    time.is_some_and(|time| from.is_none_or(|from| time >= from) && to.is_none_or(|to| time <= to))
}

#[cfg(unix)]
fn matches_permissions(path: &Path, metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    use std::os::unix::fs::PermissionsExt;

    if let Some(expected) = query.permissions.as_ref() {
        // Compare octal *values*, not strings: "000" and "0644" are modes
        // too. The special bits count, so "4755" finds a setuid file and
        // "755" does not.
        let expected_mode = u32::from_str_radix(expected.trim().trim_start_matches("0o"), 8).ok();
        if expected_mode != Some(metadata.permissions().mode() & 0o7777) {
            return false;
        }
    }
    (!query.executable || caller_can(path, libc::X_OK))
        && (!query.readable || caller_can(path, libc::R_OK))
        && (!query.writable || caller_can(path, libc::W_OK))
}

/// Whether the calling user may access `path` as `how` (`access(2)` with the
/// effective ids, through a link to its target): what the owner, group and
/// other bits grant this user, which no mode alone says.
#[cfg(unix)]
fn caller_can(path: &Path, how: libc::c_int) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a valid NUL-terminated string for the whole call, and
    // `faccessat` only reads it.
    unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), how, libc::AT_EACCESS) == 0 }
}

#[cfg(not(unix))]
fn matches_permissions(_path: &Path, metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if query.permissions.is_some() || query.executable || query.readable {
        return true;
    }
    if query.writable {
        return !metadata.permissions().readonly();
    }
    true
}

/// A file with no bytes, or a directory with no entries: the listing the walk
/// enters it by, or a read of its own when the walk will not enter it.
fn is_empty(
    path: &Path,
    metadata: &fs::Metadata,
    (depth, kind): (u32, EntryKind),
    query: &CompiledQuery,
    state: &mut QueryState,
) -> bool {
    if metadata.is_file() {
        return metadata.len() == 0;
    }
    if metadata.is_dir() {
        if kind.is_dir
            && let Some(empty) = entered_empty(path, depth, query, state)
        {
            return empty;
        }
        #[cfg(test)]
        EMPTINESS_READS.with(|reads| reads.set(reads.get() + 1));
        return fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
    }
    false
}

fn to_entry(
    path: &Path,
    name: &str,
    walk_position: u32,
    kind: EntryKind,
    metadata: Option<&fs::Metadata>,
    query: &CompiledQuery,
) -> FileSystemEntry {
    let path_string = path.to_string_lossy().to_string();
    let relative_path = path
        .strip_prefix(&query.root)
        .ok()
        .map(crate::text::slash_path)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.to_owned());
    let entry_type = if kind.is_file {
        FileKind::File
    } else if kind.is_dir {
        FileKind::Directory
    } else if kind.is_symlink {
        FileKind::Symlink
    } else {
        FileKind::Other
    };

    FileSystemEntry {
        path: path_string,
        relative_path,
        name: name.to_owned(),
        entry_type,
        size: metadata.map(|metadata| crate::num::saturating_i64(metadata.len())),
        modified_time: metadata.and_then(|metadata| metadata.modified().ok()),
        walk_position,
        stamp: metadata.and_then(crate::stamp::SourceStamp::of),
    }
}

pub(crate) fn normalize_extensions(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().trim_start_matches('.').to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}
