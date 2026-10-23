//! What one entry is listed as: its kind, the name, metadata, time and
//! permission filters it passes, and the row it becomes.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use ignore::gitignore::Gitignore;
use regex::Regex;

use super::compiled::CompiledQuery;
use super::walker::QueryState;
use crate::error::{Error, ErrorKind};
use crate::search::glob;
use crate::text::file_extension::extension_of;
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
}

impl EntryKind {
    pub(super) fn of(file_type: fs::FileType) -> Self {
        Self {
            is_dir: file_type.is_dir(),
            is_file: file_type.is_file(),
        }
    }
}

impl From<fs::FileType> for EntryKind {
    fn from(file_type: fs::FileType) -> Self {
        Self::of(file_type)
    }
}

/// `symlink_metadata`, counting a failure as a skipped entry.
pub(super) fn lstat(path: &Path, state: &mut QueryState) -> Option<fs::Metadata> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(err) => {
            state.unreadable(&err);
            None
        }
    }
}

/// Count and record `path` when it matches. Name and type filters run
/// first; the entry is stat'ed (into `metadata`) only when they pass. `Err`
/// when that stat failed: the entry is skipped like an unreadable one.
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
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => metadata.insert(lstat(path, state).ok_or(())?),
    };
    if !matches_metadata(path, metadata, query) {
        return Ok(());
    }

    state.total_discovered += 1;
    if state.entries.len() >= query.limit {
        if state.first_unlisted.is_none() {
            state.first_unlisted = Some(path.to_path_buf());
        }
        return Ok(());
    }

    state
        .entries
        .push(to_entry(path, state.position, metadata, query));
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
fn matches_metadata(path: &Path, metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if query.empty && !is_empty(path, metadata) {
        return false;
    }
    // A size filter is about content: a directory's own length (its entry
    // table) is none, so a directory never matches one.
    let sized = query.size_greater.is_some() || query.size_less.is_some();
    if sized && metadata.is_dir() {
        return false;
    }
    if let Some(min_size) = query.size_greater
        && metadata.len() <= min_size
    {
        return false;
    }
    if let Some(max_size) = query.size_less
        && metadata.len() >= max_size
    {
        return false;
    }
    if !matches_time_filters(metadata, query) {
        return false;
    }
    if !matches_permissions(metadata, query) {
        return false;
    }

    true
}

/// Whether `name` ends in one of `extensions`, normalized as a query's are
/// (lowercase, no leading dot).
pub(crate) fn has_extension(name: &str, extensions: &[String]) -> bool {
    let extension = extension_of(name, true, "");
    !extension.is_empty() && extensions.iter().any(|allowed| allowed == &extension)
}

fn matches_time_filters(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if let Some(duration) = query.modified_within_secs
        && !system_time_within(metadata.modified().ok(), duration)
    {
        return false;
    }
    if let Some(duration) = query.modified_before_secs
        && !system_time_before(metadata.modified().ok(), duration)
    {
        return false;
    }
    if let Some(duration) = query.accessed_within_secs
        && !system_time_within(metadata.accessed().ok(), duration)
    {
        return false;
    }
    true
}

fn system_time_within(time: Option<SystemTime>, seconds: u64) -> bool {
    time.and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|elapsed| elapsed.as_secs() <= seconds)
}

fn system_time_before(time: Option<SystemTime>, seconds: u64) -> bool {
    time.and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|elapsed| elapsed.as_secs() > seconds)
}

#[cfg(unix)]
fn matches_permissions(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let mode = metadata.permissions().mode() & 0o777;
    if let Some(expected) = query.permissions.as_ref() {
        // Compare octal *values*, not strings. The old `trim_start_matches('0')`
        // turned "000" into "" (so a real 000 file could never match) and also
        // mishandled forms like "0644"/"0o644". Parse both and compare numbers.
        let expected_mode = u32::from_str_radix(expected.trim().trim_start_matches("0o"), 8).ok();
        if expected_mode != Some(mode) {
            return false;
        }
    }
    if query.executable && mode & 0o111 == 0 {
        return false;
    }
    if query.readable && mode & 0o444 == 0 {
        return false;
    }
    if query.writable && mode & 0o222 == 0 {
        return false;
    }
    true
}

#[cfg(not(unix))]
fn matches_permissions(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if query.permissions.is_some() || query.executable || query.readable {
        return true;
    }
    if query.writable {
        return !metadata.permissions().readonly();
    }
    true
}

fn is_empty(path: &Path, metadata: &fs::Metadata) -> bool {
    if metadata.is_file() {
        return metadata.len() == 0;
    }
    if metadata.is_dir() {
        return fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
    }
    false
}

fn to_entry(
    path: &Path,
    walk_position: u32,
    metadata: &fs::Metadata,
    query: &CompiledQuery,
) -> FileSystemEntry {
    let path_string = path.to_string_lossy().to_string();
    let relative_path = path
        .strip_prefix(&query.root)
        .ok()
        .map(crate::text::slash_path)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| file_name(path));
    let name = file_name(path);
    let entry_type = if metadata.is_file() {
        FileKind::File
    } else if metadata.is_dir() {
        FileKind::Directory
    } else if metadata.file_type().is_symlink() {
        FileKind::Symlink
    } else {
        FileKind::Other
    };

    FileSystemEntry {
        path: path_string,
        relative_path,
        name,
        entry_type,
        size: Some(metadata.len() as i64),
        modified_time: metadata.modified().ok(),
        walk_position,
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
