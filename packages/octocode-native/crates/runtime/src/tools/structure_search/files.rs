use crate::policy::prune::DefaultsFlag;
use crate::{
    policy::{path::PathPolicy, prune::PruneMode},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::types::{FileSystemEntry, FileSystemQueryOptions};
use serde_json::{Value, json};

use crate::contracts::tool_types::{StructureSearchQueryFiles, StructureSearchQueryFilesTime};

/// Engine-unit views over the generated `files` query.
impl StructureSearchQueryFiles {
    /// Levels below `path` (1 = its children), as `tree` and the engine
    /// count them.
    fn depth(value: Option<std::num::NonZeroU64>) -> Option<u32> {
        value.map(|depth| u32::try_from(depth.get()).unwrap_or(u32::MAX))
    }
    pub fn max_depth(&self) -> Option<u32> {
        Self::depth(self.max_depth)
    }
    pub fn min_depth(&self) -> Option<u32> {
        Self::depth(self.min_depth)
    }
    fn non_empty(values: &[String]) -> Option<Vec<String>> {
        (!values.is_empty()).then(|| values.to_vec())
    }
    pub fn include(&self) -> Option<Vec<String>> {
        Self::non_empty(&self.include).map(|globs| crate::policy::include::include_globs(&globs))
    }
    pub fn extensions(&self) -> Option<Vec<String>> {
        Self::non_empty(&self.extensions)
    }
    pub fn exclude(&self) -> Option<Vec<String>> {
        Self::non_empty(&self.exclude)
    }
    pub fn entry_type(&self) -> Option<String> {
        self.entry_type.map(|kind| kind.to_string())
    }
    pub fn permissions(&self) -> Option<String> {
        self.permissions.as_ref().map(ToString::to_string)
    }
    pub fn access(&self) -> Option<String> {
        self.access.map(|access| access.to_string())
    }
    pub fn detail(&self) -> String {
        self.detail.to_string()
    }
    pub fn sort(&self) -> String {
        self.sort.to_string()
    }
    pub fn max_entries(&self) -> Option<u32> {
        self.max_entries
            .map(|limit| u32::try_from(limit.get()).unwrap_or(u32::MAX))
    }
    pub fn page(&self) -> u32 {
        u32::try_from(self.page.get()).unwrap_or(u32::MAX)
    }
    /// The caller's page size; `None` pages by the response budget.
    pub fn page_size(&self) -> Option<usize> {
        self.page_size
            .map(|size| usize::try_from(size.get()).unwrap_or(usize::MAX))
    }
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
}

struct Row {
    /// Directory the entry is listed under, named like `path`.
    dir: String,
    /// The entry text inside its directory group ([`entry_text`]).
    entry: String,
    path: String,
    /// A regular file (not a directory or symlink).
    is_file: bool,
    name: String,
    size: i64,
    modified: f64,
    lines: usize,
    /// The walked entry, for the continuation pages' change check.
    source: std::path::PathBuf,
}

/// A files walk: its rows in listing order (cut to `maxEntries`) and what
/// the walk left out.
struct FilesWalk {
    rows: Vec<Row>,
    available: usize,
    total_discovered: usize,
    was_capped: bool,
    uncovered: super::Uncovered,
    warnings: Vec<String>,
}

impl super::memo::Listed for FilesWalk {
    fn sources(&self) -> Vec<&std::path::Path> {
        self.rows.iter().map(|row| row.source.as_path()).collect()
    }
}

pub fn execute_files(
    q: &StructureSearchQueryFiles,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> super::StructureResult {
    let validated = super::validated_root(q.path.as_str(), paths, cancel)?;
    validate_time(q.time.as_ref())?;
    let requested = q
        .max_entries()
        .unwrap_or_else(super::max_walk)
        .min(super::max_walk()) as usize;
    // Snapshot fingerprint over the query shape, the page cut, and the
    // ordered result set, so a continuation cursor (page>1) can be rejected
    // with `staleSnapshot` when the corpus or query drifted
    // between pages, or a different response window would cut other pages.
    let identity = json!([
        validated.canonical.to_string_lossy(),
        q.max_depth(),
        q.min_depth(),
        q.include(),
        q.extensions(),
        q.name_regex,
        q.entry_type(),
        q.empty,
        q.permissions(),
        q.access(),
        q.exclude(),
        // Effective (not raw) values: the nextPage continuation injects these
        // defaults, so the digest must match what the follow-up request carries.
        q.sort(),
        q.detail(),
        requested,
        q.page_size(),
        q.no_ignore,
        q.default_excludes,
        q.time,
        q.size,
    ]);
    let policy = paths.identity();
    let page = q.page().max(1) as usize;
    let (walk, stored) = super::walked(page, q.snapshot(), &policy, || {
        walk_files(q, &validated.canonical, paths, security, cancel, requested)
    })?;
    let budget = super::page_budget(response_window, &identity);
    // The row layout cuts pages too, so it is part of the snapshot.
    let snapshot = crate::digest::json_sha256(&json!([
        identity,
        budget,
        ROW_LAYOUT,
        walk.rows
            .iter()
            .map(|r| (&r.dir, &r.entry, &r.path))
            .collect::<Vec<_>>()
    ]));
    if let Some(restart) = super::restart_if_stale(q, page, q.snapshot(), &snapshot) {
        return Ok(restart);
    }
    let page_size = q
        .page_size()
        .map(|size| size.clamp(1, super::structure_max("pageSize") as usize));
    let pages = super::page_ranges(&files_costs(&walk.rows), page_size, budget);
    if !stored && pages.len() > 1 {
        super::memo::put(snapshot.clone(), policy, &validated.canonical, &walk);
    }
    Ok(files_page(
        q,
        &walk,
        &pages,
        snapshot,
        &validated.canonical,
        requested,
    ))
}

/// Path order is the walk order (name-sorted, depth-first), so the first
/// `limit` matches are final once found: the walk stops there. Other sorts
/// rank the whole walk.
fn early_exit(q: &StructureSearchQueryFiles) -> bool {
    q.sort() == "path"
}

/// Walk once: rows sorted and cut to `requested`, path-ordered rows grouped
/// like `tree`, directories with their own group not also listed bare.
fn walk_files(
    q: &StructureSearchQueryFiles,
    root: &std::path::Path,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    requested: usize,
) -> Result<FilesWalk, super::StructureError> {
    let time = &q.time;
    let access = q.access();
    let access = access.as_deref();
    // Leave out what the tree and localSearch leave out: `.gitignore`d
    // entries, pruned during the walk. `defaultExcludes:false` walks them too.
    let gitignore = (q.default_excludes.defaults() && q.no_ignore != Some(true))
        .then(|| crate::policy::gitignore::GitignoreFilter::new(root));
    let probe = super::FilterProbe::new(
        root,
        &q.include().unwrap_or_default(),
        &q.extensions,
        q.name_regex.as_deref(),
    );
    let ignored = std::sync::atomic::AtomicUsize::new(0);
    let withheld = std::sync::Mutex::new(crate::policy::discovery::Withheld::default());
    let detail = q.detail();
    let sort = q.sort();
    let walk_limit = if early_exit(q) {
        requested as u32
    } else {
        super::max_walk()
    };
    let discovery = paths.discovery_walk();
    let native = octocode_engine::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            include_root: Some(true),
            recursive: Some(true),
            max_depth: q.max_depth(),
            min_depth: q.min_depth(),
            show_hidden: Some(true),
            names: q.include(),
            extensions: q.extensions(),
            path_pattern: None,
            regex: q.name_regex.clone(),
            entry_type: q.entry_type(),
            empty: q.empty,
            modified_within: time.as_ref().and_then(|t| t.modified_within.clone()),
            modified_before: time.as_ref().and_then(|t| t.modified_before.clone()),
            accessed_within: time.as_ref().and_then(|t| t.accessed_within.clone()),
            size_greater: q.size.as_ref().and_then(|s| s.greater.clone()),
            size_less: q.size.as_ref().and_then(|s| s.less.clone()),
            permissions: q.permissions(),
            executable: Some(access == Some("executable")),
            readable: Some(access == Some("readable")),
            writable: Some(access == Some("writable")),
            exclude_dir: Some(PruneMode::SyntaxVisible.directories(q.default_excludes.defaults())),
            exclude: q.exclude(),
            stop_at_limit: Some(true),
            limit: Some(walk_limit),
        },
        &|path| {
            if gitignore
                .as_ref()
                .is_some_and(|filter| filter.is_ignored(path))
            {
                ignored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(false);
            }
            let allowed = super::allow_discovery(path, &discovery, cancel)?;
            // Only an entry these filters could list makes absence unproven.
            if !allowed && probe.could_list(path) {
                withheld
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .record(path, path.is_dir());
            }
            Ok(allowed)
        },
    )
    .map_err(|error| super::walk_error(error, q.path.as_str()))?;
    cancel.check().map_err(super::cancelled)?;
    let count_lines = detail == "full" || sort == "lines";
    let mut rows = native
        .entries
        .iter()
        .map(|e| make_row(e, root, security, &detail, count_lines, paths, cancel))
        .collect::<Result<Vec<_>, super::StructureError>>()?;
    sort_rows(&mut rows, &sort);
    let available = rows.len();
    rows.truncate(requested);
    // Path order groups like `tree`: `path`'s own entries first, then each
    // directory once, in walk order (the sort is stable). Other sorts keep
    // their order, so a directory may open several groups.
    if sort == "path" {
        // The root's own `.` entry leads its bare entries.
        rows.sort_by_key(|row| {
            (
                std::path::PathBuf::from(&row.dir),
                !row.entry.starts_with("./"),
            )
        });
    }
    drop_grouped_dirs(&mut rows);
    Ok(FilesWalk {
        rows,
        available,
        total_discovered: native.total_discovered as usize,
        was_capped: native.was_capped,
        uncovered: super::Uncovered {
            ignored: ignored.into_inner(),
            withheld: withheld
                .into_inner()
                .unwrap_or_else(|error| error.into_inner()),
            hidden: 0,
            pruned: native.pruned_dirs,
        },
        warnings: {
            let mut warnings = walk_warnings(native.skipped, native.permission_denied);
            warnings.extend(native.warnings);
            warnings
        },
    })
}

/// A directory with its own `{dir, files}` group is named by that group; a
/// bare `"name/"` entry for it under its parent would show it twice, even
/// across pages. One with fields (`modifiedMs`) keeps them.
fn drop_grouped_dirs(rows: &mut Vec<Row>) {
    let grouped = super::group_dirs(rows.iter().map(|row| &row.dir));
    rows.retain(|row| {
        !(row.entry.strip_suffix('/').is_some_and(|name| name != ".")
            && grouped.contains(&row.path))
    });
}

/// A row costs its serialized entry, plus its group's header with the
/// directory rendered as the response shows it whenever it opens a group
/// (a page repeats the header of a group it continues). Costs are bound
/// by the snapshot, so every page of it cuts at the same rows.
fn files_costs(rows: &[Row]) -> Vec<super::RowCost> {
    let mut headers = std::collections::HashMap::<&str, usize>::new();
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let header = *headers.entry(row.dir.as_str()).or_insert_with(|| {
                if row.dir.is_empty() {
                    0
                } else {
                    crate::tools::stream_page::json_chars(&json!({"dir":row.dir,"files":[]})) + 1
                }
            });
            super::RowCost {
                entry: crate::tools::stream_page::json_chars(&row.entry) + 1,
                header,
                continues: index > 0 && rows[index - 1].dir == row.dir,
            }
        })
        .collect()
}

/// The response for this page of a walk.
fn files_page(
    q: &StructureSearchQueryFiles,
    walk: &FilesWalk,
    pages: &[std::ops::Range<usize>],
    snapshot: String,
    root: &std::path::Path,
    requested: usize,
) -> Value {
    let page = q.page().max(1) as usize;
    let total = walk.rows.len();
    let early_exit = early_exit(q);
    // An early-exit walk that found one more match than the limit has more;
    // only a full walk knows the total.
    let (limit_cut, scan_cut) = if early_exit {
        (walk.was_capped, false)
    } else {
        (walk.available > requested, walk.was_capped)
    };
    let cut = super::Cut {
        page,
        total_pages: pages.len().max(1),
        total,
        requested,
        available: walk.available,
        limit_cut,
        scan_cut,
        early_exit,
        total_discovered: walk.total_discovered,
        snapshot: snapshot.clone(),
    };
    let shown = pages.get(page - 1).cloned().unwrap_or(total..total);
    let files = super::dir_groups(
        &walk.rows[shown.clone()],
        |row| (&row.dir, &row.entry),
        "files",
    );
    let mut out = json!({"path":super::display_name(root),"snapshot":snapshot,"files":files,"pagination":{"currentPage":page,"totalPages":cut.total_pages,"totalItems":total,"hasMore":cut.has_more()}});
    if let Some(size) = q.page_size() {
        out["pagination"]["pageSize"] =
            json!(size.clamp(1, super::structure_max("pageSize") as usize));
    }
    if total == 0 {
        out["status"] = json!("empty")
    }
    let mut warnings = walk.warnings.clone();
    cut.finish(&mut out, q, &mut warnings);
    if page == 1
        && let Some(note) = walk.uncovered.pruned_note()
    {
        out["summary"] = json!(note);
    }
    if page == 1
        && let Some(read) = outline_read(&walk.rows[shown], root)
    {
        out["next"]["read"] = read;
    }
    if !early_exit
        && (scan_cut || walk.total_discovered > total)
        && let Some(pagination) = out.get_mut("pagination")
    {
        pagination["totalFilesFound"] = json!(walk.total_discovered)
    }
    if cut.out_of_range() {
        out["pagination"]["outOfRange"] = json!(true);
    }
    if let Some(warning) = walk.uncovered.withheld.notice() {
        if total == 0 {
            out["hints"] = json!([warning]);
        }
        warnings.push(warning);
    }
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings)
    }
    walk.uncovered.note_empty(&mut out, q);
    out
}

/// `next.read` on a listing's first page: the outline (localFetch
/// `minify:"symbols"`) of its first source or doc file, evidence the
/// listing itself does not hold. Rows name paths relative to the root.
fn outline_read(rows: &[Row], root: &std::path::Path) -> Option<Value> {
    let row = rows
        .iter()
        .find(|row| row.is_file && crate::tools::outlines(&row.path))?;
    let path = if row.path.is_empty() || root.is_file() {
        root.to_path_buf()
    } else {
        root.join(&row.path)
    };
    Some(
        crate::tools::result::Continuation::new(
            crate::tools::id::ToolId::LocalFetch,
            json!({"path": path.to_string_lossy(), "minify": "symbols"}),
        )
        .build(),
    )
}

fn make_row(
    e: &FileSystemEntry,
    root: &std::path::Path,
    security: &ContentSecurity,
    detail: &str,
    count_lines: bool,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Row, super::StructureError> {
    let full = detail == "full";
    // Rows name paths relative to the walked root, like `tree`: `""` is the
    // root itself, which a directory root lists as `.`.
    let is_root = e.relative_path.is_empty() || std::path::Path::new(&e.path) == root;
    let kind = match e.entry_type.as_str() {
        "directory" => "directory",
        "symlink" => "symlink",
        _ => "file",
    };
    let relative = if is_root && kind != "directory" {
        root.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        e.relative_path.clone()
    };
    let path = security.sanitize_text(&relative, None).content;
    let (dir, name) = if is_root && kind == "directory" {
        ("", ".")
    } else {
        path.rsplit_once('/').unwrap_or(("", path.as_str()))
    };
    let dir = dir.to_owned();
    let modified = e.modified_ms.unwrap_or(0.0);
    let lines = if count_lines && kind != "directory" {
        line_count(std::path::Path::new(&e.path), paths, cancel)?
    } else {
        0
    };
    let entry = entry_text(
        name,
        kind,
        e.size.filter(|_| kind != "directory"),
        (full && lines > 0).then_some(lines),
        // Whole epoch milliseconds: a float would print as `…737.0`.
        e.modified_ms
            .filter(|_| full || detail == "modified")
            .map(|modified| modified.round() as i64),
    );
    Ok(Row {
        dir,
        entry,
        path,
        is_file: kind == "file",
        name: e.name.clone(),
        size: e.size.unwrap_or(0),
        modified,
        lines,
        source: std::path::PathBuf::from(&e.path),
    })
}
/// Page layout tag folded into the snapshot: rows grouped by directory.
const ROW_LAYOUT: &str = "pathRelativeDirGroups";

/// One listed entry inside its directory group: the name, `/` after a
/// directory, then ` (<fields>)` naming the size in bytes (every non-directory
/// entry), `symlink`, `lineCount=N` and `modifiedMs=N` as they apply. The
/// fields never contain ` (`, so the last ` (` of an entry ending in `)`
/// opens them, whatever the name holds.
fn entry_text(
    name: &str,
    kind: &str,
    size: Option<i64>,
    line_count: Option<usize>,
    modified_ms: Option<i64>,
) -> String {
    let mut text = name.to_owned();
    if kind == "directory" {
        text.push('/');
    }
    let mut fields = Vec::new();
    if let Some(size) = size {
        fields.push(size.to_string());
    }
    if kind == "symlink" {
        fields.push("symlink".to_owned());
    }
    if let Some(lines) = line_count {
        fields.push(format!("lineCount={lines}"));
    }
    if let Some(modified) = modified_ms {
        fields.push(format!("modifiedMs={modified}"));
    }
    if !fields.is_empty() {
        text.push_str(&format!(" ({})", fields.join(", ")));
    }
    text
}

fn sort_rows(r: &mut [Row], sort: &str) {
    r.sort_by(|a, b| match sort {
        "lines" => b.lines.cmp(&a.lines),
        "size" => b.size.cmp(&a.size),
        "name" => a.name.cmp(&b.name),
        // Component-wise, the walk order: `a/x` before `a-b/x`.
        "path" => std::path::Path::new(&a.path).cmp(std::path::Path::new(&b.path)),
        "modified" => b.modified.total_cmp(&a.modified),
        _ => std::path::Path::new(&a.path).cmp(std::path::Path::new(&b.path)),
    })
}
fn line_count(
    path: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<usize, super::StructureError> {
    use std::io::Read;
    cancel.check().map_err(super::cancelled)?;
    // Revalidate immediately before reading: discovery authorization is not
    // permission to follow a subsequently changed link or open a special file.
    let Ok(validated) = paths.validate_read(path) else {
        return Ok(0);
    };
    let Ok(mut file) = std::fs::File::open(validated.canonical) else {
        return Ok(0);
    };
    let mut buffer = [0_u8; 16 * 1024];
    // Newlines terminate lines; a non-empty final line without one still counts.
    let mut lines = 0_usize;
    let mut last_byte = None;
    loop {
        cancel.check().map_err(super::cancelled)?;
        match file.read(&mut buffer) {
            Ok(0) => return Ok(lines + usize::from(last_byte.is_some_and(|b| b != b'\n'))),
            Ok(bytes) => {
                lines += buffer[..bytes]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count();
                last_byte = buffer[..bytes].last().copied();
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Ok(0),
        }
    }
}
pub(super) fn format_size(n: i64) -> String {
    let b = n as f64;
    if n < 1024 {
        format!("{n}B")
    } else if n < 1_048_576 {
        format!("{:.1}KB", b / 1024.)
    } else if n < 1_073_741_824 {
        format!("{:.1}MB", b / 1_048_576.)
    } else if n < 1_099_511_627_776 {
        format!("{:.1}GB", b / 1_073_741_824.)
    } else {
        format!("{:.1}TB", b / 1_099_511_627_776.)
    }
}
fn validate_time(
    time: Option<&StructureSearchQueryFilesTime>,
) -> Result<(), super::StructureError> {
    let Some(t) = time else {
        return Ok(());
    };
    for (key, value) in [
        ("modifiedWithin", &t.modified_within),
        ("modifiedBefore", &t.modified_before),
        ("accessedWithin", &t.accessed_within),
    ] {
        if let Some(value) = value.as_deref().filter(|value| !valid_duration(value)) {
            return Err(super::StructureError::new(
                "invalidInput",
                format!(
                    "time.{key}=\"{value}\" has an unsupported format. Use a relative duration like \"7d\", \"2h\", \"1w\", or \"3m\"."
                ),
            ));
        }
    }
    Ok(())
}
fn valid_duration(v: &str) -> bool {
    v.len() > 1
        && matches!(v.chars().last(), Some('h' | 'd' | 'w' | 'm'))
        && v[..v.len() - 1].chars().all(|c| c.is_ascii_digit())
}
pub(super) fn walk_warnings(s: u32, d: u32) -> Vec<String> {
    if s == 0 {
        return vec![];
    }
    let o = s.saturating_sub(d);
    vec![if d > 0 && o > 0 {
        format!("{s} entries skipped ({d} permission denied, {o} other errors)")
    } else if d > 0 {
        format!(
            "{d} {} skipped due to permission denied",
            if d == 1 { "entry" } else { "entries" }
        )
    } else {
        format!(
            "{s} {} skipped due to access errors",
            if s == 1 { "entry" } else { "entries" }
        )
    }]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formatting() {
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(162), "162B");
    }

    #[test]
    fn line_count_can_be_cancelled_between_bounded_reads() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Cancel(AtomicUsize);
        impl CancellationCheck for Cancel {
            fn check(&self) -> Result<(), String> {
                if self.0.fetch_add(1, Ordering::SeqCst) >= 3 {
                    Err("stop line count".into())
                } else {
                    Ok(())
                }
            }
        }
        let root =
            std::env::temp_dir().join(format!("octocode-structure-lines-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("fixture");
        let file = root.join("large.rs");
        std::fs::write(&file, "line\n".repeat(20_000)).expect("large source");
        let paths = crate::tools::test_support::workspace_policy(&root);
        let error = line_count(&file, &paths, &Cancel(AtomicUsize::new(0)))
            .expect_err("cancel while reading");
        assert_eq!(error.code, "structure.execution.cancelled");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
