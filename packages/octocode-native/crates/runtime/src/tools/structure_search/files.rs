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
    fn depth(value: Option<i64>) -> Option<u32> {
        value.map(|depth| u32::try_from(depth.max(0)).unwrap_or(u32::MAX))
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
    pub fn names(&self) -> Option<Vec<String>> {
        Self::non_empty(&self.names)
    }
    pub fn extensions(&self) -> Option<Vec<String>> {
        Self::non_empty(&self.extensions)
    }
    pub fn exclude_dir(&self) -> Option<Vec<String>> {
        Self::non_empty(&self.exclude_dir)
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
    pub fn limit(&self) -> Option<u32> {
        self.limit
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

/// Estimated JSON bytes of a basic row besides its path
/// (`{"path":"","size":123456},`).
const ROW_BYTES: usize = 30;
/// Extra bytes `detail:"full"` rows carry (`modifiedMs`, `lineCount`).
const FULL_DETAIL_BYTES: usize = 46;
/// Extra bytes `detail:"modified"` rows carry (`modifiedMs`).
const MODIFIED_DETAIL_BYTES: usize = 30;

struct Row {
    output: Value,
    path: String,
    name: String,
    size: i64,
    modified: f64,
    lines: usize,
}

pub fn execute_files(
    q: &StructureSearchQueryFiles,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::StructureResult {
    cancel.check().map_err(super::cancelled)?;
    let validated = paths
        .validate(q.path.as_str())
        .map_err(super::StructureError::from)?;
    validate_time(q.time.as_ref())?;
    let time = &q.time;
    let access = q.access();
    let access = access.as_deref();
    // Leave out what the tree and localSearch leave out: `.gitignore`d
    // entries, pruned during the walk. `defaultExcludes:false` walks them too.
    let gitignore = (q.default_excludes.defaults() && q.no_ignore != Some(true))
        .then(|| crate::policy::gitignore::GitignoreFilter::new(&validated.canonical));
    let ignored = std::sync::atomic::AtomicUsize::new(0);
    let detail = q.detail();
    let sort = q.sort();
    let requested = q
        .limit()
        .unwrap_or_else(super::max_walk)
        .min(super::max_walk()) as usize;
    // Path order is the walk order (name-sorted, depth-first), so the first
    // `limit` matches are final once found: stop the walk there. Other sorts
    // rank the whole walk.
    let early_exit = sort == "path";
    let walk_limit = if early_exit {
        requested as u32
    } else {
        super::max_walk()
    };
    let native = octocode_engine::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            include_root: Some(true),
            recursive: Some(true),
            max_depth: q.max_depth(),
            min_depth: q.min_depth(),
            show_hidden: Some(true),
            names: q.names(),
            extensions: q.extensions(),
            path_pattern: q.path_pattern.clone(),
            regex: q.path_regex.clone(),
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
            exclude_dir: Some(
                PruneMode::SyntaxVisible.directories(&q.exclude_dir, q.default_excludes.defaults()),
            ),
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
            super::allow_discovery(path, paths, cancel)
        },
    )
    .map_err(super::walk_error)?;
    cancel.check().map_err(super::cancelled)?;
    let mut warnings = walk_warnings(native.skipped, native.permission_denied);
    let count_lines = detail == "full" || sort == "lines";
    let mut rows = native
        .entries
        .iter()
        .map(|e| {
            make_row(
                e,
                &validated.canonical,
                security,
                &detail,
                count_lines,
                paths,
                cancel,
            )
        })
        .collect::<Result<Vec<_>, super::StructureError>>()?;
    sort_rows(&mut rows, &sort);
    let available = rows.len();
    rows.truncate(requested);
    let total = rows.len();
    // Snapshot fingerprint over the query shape plus the ordered result set, so
    // a continuation cursor (page>1) can be rejected with `structure.snapshot.changed`
    // when the corpus or query drifted between pages.
    let snapshot = super::digest(&json!([
        q.path,
        q.max_depth(),
        q.min_depth(),
        q.names(),
        q.extensions(),
        q.path_pattern,
        q.path_regex,
        q.entry_type(),
        q.empty,
        q.permissions(),
        q.access(),
        q.exclude_dir(),
        // Effective (not raw) values: the nextPage continuation injects these
        // defaults, so the digest must match what the follow-up request carries.
        q.sort(),
        q.detail(),
        requested,
        rows.iter().map(|r| &r.path).collect::<Vec<_>>()
    ]));
    if q.page() > 1 && q.snapshot() != Some(snapshot.as_str()) {
        return Ok(super::snapshot_changed(q, &snapshot));
    }
    let page_size = q
        .page_size()
        .map(|size| size.clamp(1, super::structure_max("pageSize") as usize));
    // A row's cost comes from its path and the requested detail, both bound
    // by the snapshot, so every page of it cuts at the same rows.
    let root = super::rendered_root_bytes(paths, &validated.canonical);
    let root_name = super::display_name(&validated.canonical).len();
    let row_bytes = ROW_BYTES
        + match detail.as_str() {
            "full" => FULL_DETAIL_BYTES,
            "modified" => MODIFIED_DETAIL_BYTES,
            _ => 0,
        };
    let costs = rows
        .iter()
        .map(|row| row.path.len().saturating_sub(root_name) + root + row_bytes)
        .collect::<Vec<_>>();
    let pages = super::page_ranges(&costs, page_size);
    let page = q.page().max(1) as usize;
    let total_pages = pages.len().max(1);
    let shown = pages.get(page - 1).cloned().unwrap_or(total..total);
    let files = rows[shown]
        .iter()
        .map(|r| r.output.clone())
        .collect::<Vec<_>>();
    let out_of_range = total > 0 && page > total_pages;
    let has_more = page < total_pages;
    // An early-exit walk that found one more match than the limit has more;
    // only a full walk knows the total.
    let (limit_cut, scan_cut) = if early_exit {
        (native.was_capped, false)
    } else {
        (available > total, native.was_capped)
    };
    let can_expand = limit_cut && requested < super::max_walk() as usize;
    let terminal = (has_more && page >= 1000) || ((limit_cut || scan_cut) && !can_expand);
    let mut out = json!({"path":super::display_name(&validated.canonical),"snapshot":snapshot,"files":files,"pagination":{"currentPage":page,"totalPages":total_pages,"totalFiles":total,"hasMore":has_more}});
    if let Some(size) = page_size {
        out["pagination"]["filesPerPage"] = json!(size);
    }
    if total == 0 {
        out["status"] = json!("empty")
    }
    if has_more && !terminal {
        out["pagination"]["nextPage"] = json!(page + 1);
        out["next"]["nextPage"] = super::continuation(q, json!({"page":page+1,"snapshot":snapshot}))
    }
    if can_expand {
        out["next"]["expandLimit"] = super::continuation(
            q,
            json!({"limit":requested.saturating_mul(2).max(requested+1).min(super::max_walk() as usize),"page":1}),
        )
    }
    if terminal {
        out["terminalLimit"] = json!(true)
    }
    if limit_cut || scan_cut {
        let mut reasons = vec![];
        if limit_cut {
            reasons.push("limit")
        }
        if scan_cut {
            reasons.push("walkLimit")
        }
        out["truncated"] = json!(true);
        out["partialReasons"] = json!(reasons);
        if early_exit {
            out["atLeast"] = json!(total + 1);
        } else {
            out["totalAvailable"] = json!(usize::max(native.total_discovered as usize, available));
        }
    }
    if !early_exit
        && (scan_cut || native.total_discovered as usize > total)
        && let Some(pagination) = out.get_mut("pagination")
    {
        pagination["totalFilesFound"] = json!(native.total_discovered)
    }
    if out_of_range {
        out["pagination"]["outOfRange"] = json!(true);
        warnings.push(format!("page:{page} is out of range (only {total_pages} page(s), {total} total file(s)) — returned 0 files. Use page:1..{total_pages}."));
    }
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings)
    }
    let ignored = ignored.into_inner();
    super::note_ignored_empty(
        &mut out,
        q,
        ignored,
        json!({"noIgnore":true,"page":1}),
        format!(
            "{ignored} entries here are .gitignore'd; retry with noIgnore:true (next.includeIgnored) to list them."
        ),
    );
    Ok(out)
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
    // Rows name the root's own directory first; the envelope anchors them on
    // the root's parent, so every row path resolves against the workspace.
    let root_name = root.file_name().unwrap_or_default().to_string_lossy();
    let relative = if e.relative_path.is_empty() || std::path::Path::new(&e.path) == root {
        root_name.into_owned()
    } else {
        format!("{root_name}/{}", e.relative_path)
    };
    let path = security.sanitize_text(&relative, None).content;
    let kind = match e.entry_type.as_str() {
        "directory" => "directory",
        "symlink" => "symlink",
        _ => "file",
    };
    // `type` is omitted for regular files (the common case); only directories
    // and symlinks carry it.
    let mut output = json!({"path":path});
    if kind != "file" {
        output["type"] = json!(kind);
    }
    // Bytes as a number; the human-readable form is a debug field.
    if kind != "directory"
        && let Some(size) = e.size
    {
        output["size"] = json!(size);
        output["sizeFormatted"] = json!(format_size(size));
    }
    let modified = e.modified_ms.unwrap_or(0.0);
    if (full || detail == "modified")
        && let Some(modified) = e.modified_ms
    {
        // Whole epoch milliseconds: a float would print as `…737.0`.
        output["modifiedMs"] = json!(modified.round() as i64);
    }
    let lines = if count_lines && kind != "directory" {
        line_count(std::path::Path::new(&e.path), paths, cancel)?
    } else {
        0
    };
    if full && lines > 0 {
        output["lineCount"] = json!(lines)
    }
    Ok(Row {
        output,
        path,
        name: e.name.clone(),
        size: e.size.unwrap_or(0),
        modified,
        lines,
    })
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
        use crate::policy::path::PathPolicyConfig;
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
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        let error = line_count(&file, &paths, &Cancel(AtomicUsize::new(0)))
            .expect_err("cancel while reading");
        assert_eq!(error.code, "structure.execution.cancelled");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
