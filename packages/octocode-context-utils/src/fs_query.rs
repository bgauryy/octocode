use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::file_extension::get_extension_internal;
use crate::types::{FileSystemEntry, FileSystemQueryOptions, FileSystemQueryResult};

const DEFAULT_LIMIT: usize = 10_000;

struct CompiledQuery {
    root: PathBuf,
    include_root: bool,
    recursive: bool,
    max_depth: Option<u32>,
    min_depth: u32,
    show_hidden: bool,
    name_globs: Vec<Regex>,
    path_glob: Option<Regex>,
    regex: Option<Regex>,
    entry_type: Option<String>,
    empty: bool,
    modified_within_secs: Option<u64>,
    modified_before_secs: Option<u64>,
    accessed_within_secs: Option<u64>,
    size_greater: Option<u64>,
    size_less: Option<u64>,
    permissions: Option<String>,
    executable: bool,
    readable: bool,
    writable: bool,
    exclude_dir: Vec<String>,
    limit: usize,
    warnings: Vec<String>,
}

#[derive(Default)]
struct QueryState {
    entries: Vec<FileSystemEntry>,
    total_discovered: u32,
    skipped: u32,
    permission_denied: u32,
}

pub(crate) fn query_file_system_inner(
    options: FileSystemQueryOptions,
) -> Result<FileSystemQueryResult, String> {
    let query = CompiledQuery::new(options)?;
    let mut state = QueryState::default();
    let root_metadata = fs::symlink_metadata(&query.root).map_err(|err| {
        format!(
            "Cannot access filesystem query root '{}': {err}",
            query.root.display()
        )
    })?;

    if query.include_root {
        visit_path_with_metadata(&query.root, 0, root_metadata.clone(), &query, &mut state);
    }

    if root_metadata.is_dir() && (query.recursive || !query.include_root) {
        walk_children(&query.root, 1, &query, &mut state);
    } else if !root_metadata.is_dir() && !query.include_root {
        return Err(format!(
            "Filesystem query root is not a directory: {}",
            query.root.display()
        ));
    }

    let was_capped = state.total_discovered as usize > query.limit;
    Ok(FileSystemQueryResult {
        entries: state.entries,
        total_discovered: state.total_discovered,
        was_capped,
        skipped: state.skipped,
        permission_denied: state.permission_denied,
        warnings: query.warnings,
    })
}

impl CompiledQuery {
    fn new(options: FileSystemQueryOptions) -> Result<Self, String> {
        let root = PathBuf::from(options.path);
        let mut warnings = Vec::new();
        let name_globs = compile_globs(options.names.unwrap_or_default(), "names", &mut warnings);
        let path_glob = match options.path_pattern {
            Some(pattern) => Some(
                compile_glob(&pattern, "pathPattern")
                    .map_err(|err| err.reason)?,
            ),
            None => None,
        };
        let regex = match options.regex {
            Some(pattern) => Some(Regex::new(&pattern).map_err(|err| {
                format!("Invalid regex for local filesystem query: {err}")
            })?),
            None => None,
        };

        Ok(Self {
            root,
            include_root: options.include_root.unwrap_or(false),
            recursive: options.recursive.unwrap_or(true),
            max_depth: options.max_depth,
            min_depth: options.min_depth.unwrap_or(0),
            show_hidden: options.show_hidden.unwrap_or(true),
            name_globs,
            path_glob,
            regex,
            entry_type: options.entry_type,
            empty: options.empty.unwrap_or(false),
            modified_within_secs: parse_duration_option(
                options.modified_within.as_deref(),
                "modifiedWithin",
                &mut warnings,
            ),
            modified_before_secs: parse_duration_option(
                options.modified_before.as_deref(),
                "modifiedBefore",
                &mut warnings,
            ),
            accessed_within_secs: parse_duration_option(
                options.accessed_within.as_deref(),
                "accessedWithin",
                &mut warnings,
            ),
            size_greater: parse_size_option(options.size_greater.as_deref(), "sizeGreater")?,
            size_less: parse_size_option(options.size_less.as_deref(), "sizeLess")?,
            permissions: options.permissions,
            executable: options.executable.unwrap_or(false),
            readable: options.readable.unwrap_or(false),
            writable: options.writable.unwrap_or(false),
            exclude_dir: options.exclude_dir.unwrap_or_default(),
            limit: options.limit.map(|n| n as usize).unwrap_or(DEFAULT_LIMIT),
            warnings,
        })
    }
}

fn walk_children(base: &Path, depth: u32, query: &CompiledQuery, state: &mut QueryState) {
    if query.max_depth.is_some_and(|max_depth| depth > max_depth) {
        return;
    }

    let read_dir = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(err) => {
            state.skipped += 1;
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                state.permission_denied += 1;
            }
            return;
        }
    };

    for dir_entry in read_dir {
        let dir_entry = match dir_entry {
            Ok(entry) => entry,
            Err(err) => {
                state.skipped += 1;
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    state.permission_denied += 1;
                }
                continue;
            }
        };

        let path = dir_entry.path();
        let file_name = dir_entry.file_name();
        let name = file_name.to_string_lossy();
        if !query.show_hidden && name.starts_with('.') {
            continue;
        }

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => {
                state.skipped += 1;
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    state.permission_denied += 1;
                }
                continue;
            }
        };

        let is_directory = metadata.is_dir();
        if is_directory && query.exclude_dir.iter().any(|dir| dir == name.as_ref()) {
            continue;
        }

        visit_path_with_metadata(&path, depth, metadata, query, state);

        if query.recursive && is_directory {
            walk_children(&path, depth + 1, query, state);
        }
    }
}

fn visit_path_with_metadata(
    path: &Path,
    depth: u32,
    metadata: fs::Metadata,
    query: &CompiledQuery,
    state: &mut QueryState,
) {
    if depth < query.min_depth {
        return;
    }
    if query.max_depth.is_some_and(|max_depth| depth > max_depth) {
        return;
    }
    if !matches_query(path, &metadata, query) {
        return;
    }

    state.total_discovered += 1;
    if state.entries.len() >= query.limit {
        return;
    }

    state
        .entries
        .push(to_entry(path, depth.saturating_sub(1), &metadata, query));
}

fn matches_query(path: &Path, metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    let name = file_name(path);
    let normalized_path = normalize_path(path);

    if !query.name_globs.is_empty() && !query.name_globs.iter().any(|re| re.is_match(&name)) {
        return false;
    }
    if let Some(path_glob) = &query.path_glob {
        if !path_glob.is_match(&normalized_path) {
            return false;
        }
    }
    if let Some(regex) = &query.regex {
        if !regex.is_match(&name) {
            return false;
        }
    }
    if let Some(entry_type) = &query.entry_type {
        let matches_type = match entry_type.as_str() {
            "f" => metadata.is_file(),
            "d" => metadata.is_dir(),
            "l" => metadata.file_type().is_symlink(),
            _ => true,
        };
        if !matches_type {
            return false;
        }
    }
    if query.empty && !is_empty(path, metadata) {
        return false;
    }
    if let Some(min_size) = query.size_greater {
        if metadata.len() <= min_size {
            return false;
        }
    }
    if let Some(max_size) = query.size_less {
        if metadata.len() >= max_size {
            return false;
        }
    }
    if !matches_time_filters(metadata, query) {
        return false;
    }
    if !matches_permissions(metadata, query) {
        return false;
    }

    true
}

fn matches_time_filters(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if let Some(duration) = query.modified_within_secs {
        if !system_time_within(metadata.modified().ok(), duration) {
            return false;
        }
    }
    if let Some(duration) = query.modified_before_secs {
        if !system_time_before(metadata.modified().ok(), duration) {
            return false;
        }
    }
    if let Some(duration) = query.accessed_within_secs {
        if !system_time_within(metadata.accessed().ok(), duration) {
            return false;
        }
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
    if query
        .permissions
        .as_ref()
        .is_some_and(|expected| format!("{mode:03o}") != expected.trim_start_matches('0'))
    {
        return false;
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
    output_depth: u32,
    metadata: &fs::Metadata,
    query: &CompiledQuery,
) -> FileSystemEntry {
    let path_string = path.to_string_lossy().to_string();
    let relative_path = path
        .strip_prefix(&query.root)
        .ok()
        .map(path_to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| file_name(path));
    let name = file_name(path);
    let entry_type = if metadata.is_file() {
        "file"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.file_type().is_symlink() {
        "symlink"
    } else {
        "other"
    }
    .to_owned();

    FileSystemEntry {
        path: path_string,
        relative_path,
        name: name.clone(),
        entry_type,
        size: Some(metadata.len() as i64),
        modified_ms: metadata.modified().ok().and_then(system_time_to_ms),
        accessed_ms: metadata.accessed().ok().and_then(system_time_to_ms),
        permissions: permission_string(metadata),
        extension: Some(get_extension_internal(&name, false, "")),
        depth: output_depth,
    }
}

fn system_time_to_ms(time: SystemTime) -> Option<f64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as f64)
}

#[cfg(unix)]
fn permission_string(metadata: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    Some(format!("{:03o}", metadata.permissions().mode() & 0o777))
}

#[cfg(not(unix))]
fn permission_string(_metadata: &fs::Metadata) -> Option<String> {
    None
}

fn parse_duration_option(
    value: Option<&str>,
    label: &str,
    warnings: &mut Vec<String>,
) -> Option<u64> {
    match value {
        None => None,
        Some(raw) => match parse_duration(raw) {
            Some(seconds) => Some(seconds),
            None => {
                warnings.push(format!("{label} skipped: invalid duration format '{raw}'"));
                None
            }
        },
    }
}

fn parse_duration(raw: &str) -> Option<u64> {
    let (number, unit) = raw.split_at(raw.len().saturating_sub(1));
    let value = number.parse::<u64>().ok()?;
    match unit {
        "m" => Some(value * 60),
        "h" => Some(value * 60 * 60),
        "d" => Some(value * 24 * 60 * 60),
        "w" => Some(value * 7 * 24 * 60 * 60),
        _ => None,
    }
}

fn parse_size_option(value: Option<&str>, label: &str) -> Result<Option<u64>, String> {
    value
        .map(|raw| {
            parse_size(raw).ok_or_else(|| {
                format!("Invalid {label} value for local filesystem query: {raw}")
            })
        })
        .transpose()
}

fn parse_size(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let split_at = trimmed
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit() && *ch != '.')
        .map(|(idx, _)| idx)
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split_at);
    let value = number.parse::<f64>().ok()?;
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "" | "b" | "c" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * multiplier).round() as u64)
}

fn compile_globs(values: Vec<String>, label: &str, warnings: &mut Vec<String>) -> Vec<Regex> {
    values
        .into_iter()
        .map(|value| compile_glob(&value, label))
        .filter_map(|result| match result {
            Ok(regex) => Some(regex),
            Err(err) => {
                warnings.push(err.reason);
                None
            }
        })
        .collect()
}

struct GlobError {
    reason: String,
}

fn compile_glob(pattern: &str, label: &str) -> std::result::Result<Regex, GlobError> {
    let mut out = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            _ => out.push_str(&regex::escape(&ch.to_string())),
        }
    }
    out.push('$');
    Regex::new(&out).map_err(|err| GlobError {
        reason: format!("{label} glob skipped: invalid pattern '{pattern}' ({err})"),
    })
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("octocode_fs_query_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn finds_files_by_name_glob() {
        let root = temp_root("glob");
        File::create(root.join("a.ts")).expect("create a.ts");
        File::create(root.join("b.js")).expect("create b.js");

        let result = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            names: Some(vec!["*.ts".to_owned()]),
            ..Default::default()
        })
        .expect("query");

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].name, "a.ts");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn respects_depth_and_exclude_dir() {
        let root = temp_root("depth");
        fs::create_dir_all(root.join("src/nested")).expect("create src");
        fs::create_dir_all(root.join("node_modules/pkg")).expect("create node_modules");
        File::create(root.join("src/nested/a.ts")).expect("create nested");
        File::create(root.join("node_modules/pkg/index.js")).expect("create ignored");

        let result = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            max_depth: Some(3),
            exclude_dir: Some(vec!["node_modules".to_owned()]),
            names: Some(vec!["*.ts".to_owned()]),
            ..Default::default()
        })
        .expect("query");

        assert_eq!(result.entries.len(), 1);
        assert!(result.entries[0].path.ends_with("src/nested/a.ts"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn filters_by_size_and_empty() {
        let root = temp_root("size");
        File::create(root.join("empty.txt")).expect("create empty");
        fs::write(root.join("full.txt"), "hello").expect("write full");

        let empty = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            empty: Some(true),
            names: Some(vec!["*.txt".to_owned()]),
            ..Default::default()
        })
        .expect("query empty");
        assert_eq!(empty.entries.len(), 1);
        assert_eq!(empty.entries[0].name, "empty.txt");

        let full = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            size_greater: Some("1b".to_owned()),
            names: Some(vec!["*.txt".to_owned()]),
            ..Default::default()
        })
        .expect("query full");
        assert_eq!(full.entries.len(), 1);
        assert_eq!(full.entries[0].name, "full.txt");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn filters_by_entry_type() {
        let root = temp_root("entry_type");
        fs::create_dir_all(root.join("src")).expect("create src");
        File::create(root.join("src/file.rs")).expect("create file");

        let result = query_file_system_inner(FileSystemQueryOptions {
            path: root.to_string_lossy().to_string(),
            entry_type: Some("d".to_owned()),
            ..Default::default()
        })
        .expect("query dirs");

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].name, "src");
        assert_eq!(result.entries[0].entry_type, "directory");
        fs::remove_dir_all(root).expect("cleanup");
    }
}
