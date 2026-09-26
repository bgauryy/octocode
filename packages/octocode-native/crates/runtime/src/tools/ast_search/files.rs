use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine::types::{FileSystemEntry, FileSystemQueryOptions};
use serde_json::{Value, json};

const MAX_WALK: u32 = 10_000;
pub use crate::contracts::tool_types::{AstSearchQueryFiles, AstSearchQueryFilesTime};

/// Engine-unit views over the generated `files` query.
impl AstSearchQueryFiles {
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
    pub fn page_size(&self) -> u32 {
        self.page_size
            .map_or(100, |size| u32::try_from(size.get()).unwrap_or(u32::MAX))
    }
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
}

struct Row {
    output: Value,
    path: String,
    name: String,
    size: i64,
    modified: f64,
    lines: usize,
}

pub fn execute_files(
    q: &AstSearchQueryFiles,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    let validated = paths
        .validate(q.path.as_str())
        .map_err(super::AstError::from)?;
    let (time, mut warnings) = valid_time(q.time.clone());
    let access = q.access();
    let access = access.as_deref();
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
            exclude_dir: q.exclude_dir(),
            stop_at_limit: Some(true),
            limit: Some(MAX_WALK),
        },
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::native_error)?;
    cancel.check().map_err(super::cancelled)?;
    warnings.extend(walk_warnings(native.skipped, native.permission_denied));
    let full = q.detail() == "full";
    // Modification times are collected for the `modified` sort (the schema
    // default, newest first; ties keep walk order) and `detail` modified/full.
    let collect_modified = full || q.detail() == "modified" || q.sort() == "modified";
    let count_lines = (full || q.sort() == "lines") && native.entries.len() <= 2_000;
    let mut rows = native
        .entries
        .iter()
        .map(|e| {
            make_row(
                e,
                &validated.canonical,
                security,
                full,
                count_lines,
                paths,
                cancel,
            )
        })
        .collect::<Result<Vec<_>, super::AstError>>()?;
    sort_rows(&mut rows, &q.sort(), collect_modified);
    let available = rows.len();
    let requested = q.limit().unwrap_or(MAX_WALK).min(MAX_WALK) as usize;
    rows.truncate(requested);
    let total = rows.len();
    // Snapshot fingerprint over the query shape plus the ordered result set, so
    // a continuation cursor (page>1) can be rejected with `ast.snapshot.changed`
    // when the corpus or query drifted between pages.
    let snapshot = super::syntax::digest(&json!([
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
        return Ok(super::snapshot_changed(&snapshot));
    }
    let page_size = q.page_size().clamp(1, 100) as usize;
    let page = q.page().max(1) as usize;
    let total_pages = total.div_ceil(page_size).max(1);
    let start = (page - 1).saturating_mul(page_size);
    let files = rows
        .get(start..start.saturating_add(page_size).min(total))
        .unwrap_or(&[])
        .iter()
        .map(|r| r.output.clone())
        .collect::<Vec<_>>();
    let out_of_range = total > 0 && start >= total;
    let has_more = page < total_pages;
    let limit_cut = available > total;
    let scan_cut = native.was_capped;
    let can_expand = limit_cut && requested < MAX_WALK as usize;
    let terminal = (has_more && page >= 1000) || ((limit_cut || scan_cut) && !can_expand);
    let mut out = json!({"path":super::display_name(&validated.canonical),"snapshot":snapshot,"files":files,"pagination":{"currentPage":page,"totalPages":total_pages,"filesPerPage":page_size,"totalFiles":total,"hasMore":has_more}});
    if total == 0 {
        out["status"] = json!("empty")
    }
    if has_more && !terminal {
        out["pagination"]["nextPage"] = json!(page + 1);
        out["next"]["nextPage"] = continuation(q, json!({"page":page+1,"snapshot":snapshot}))
    }
    if can_expand {
        out["next"]["expandLimit"] = continuation(
            q,
            json!({"limit":requested.saturating_mul(2).max(requested+1).min(MAX_WALK as usize),"page":1}),
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
        out["totalAvailable"] = json!(usize::max(native.total_discovered as usize, available));
    }
    if (scan_cut || native.total_discovered as usize > total)
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
    Ok(out)
}
fn make_row(
    e: &FileSystemEntry,
    root: &std::path::Path,
    security: &ContentSecurity,
    full: bool,
    count_lines: bool,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Row, super::AstError> {
    let root_name = root.file_name().unwrap_or_default().to_string_lossy();
    let relative = if e.relative_path == root_name || e.relative_path.is_empty() {
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
    if kind != "directory"
        && let Some(size) = e.size
    {
        if full {
            output["size"] = json!(size)
        } else {
            output["sizeFormatted"] = json!(format_size(size))
        }
    }
    let modified = e.modified_ms.unwrap_or(0.0);
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
fn continuation(q: &AstSearchQueryFiles, changes: Value) -> Value {
    let mut query = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let (Some(to), Some(from)) = (query.as_object_mut(), changes.as_object()) {
        to.extend(from.clone())
    }
    json!({"tool":"astSearch","query":query,"confidence":"exact"})
}
fn sort_rows(r: &mut [Row], sort: &str, modified: bool) {
    r.sort_by(|a, b| match sort {
        "lines" => b.lines.cmp(&a.lines),
        "size" => b.size.cmp(&a.size),
        "name" => a.name.cmp(&b.name),
        "path" => a.path.cmp(&b.path),
        _ if modified => b.modified.total_cmp(&a.modified),
        _ => a.path.cmp(&b.path),
    })
}
fn line_count(
    path: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<usize, super::AstError> {
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
        format!("{n}.0B")
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
fn valid_time(
    time: Option<AstSearchQueryFilesTime>,
) -> (Option<AstSearchQueryFilesTime>, Vec<String>) {
    let Some(mut t) = time else {
        return (None, vec![]);
    };
    let mut w = vec![];
    for (key, value) in [
        ("modifiedWithin", &mut t.modified_within),
        ("modifiedBefore", &mut t.modified_before),
        ("accessedWithin", &mut t.accessed_within),
    ] {
        if value.as_deref().is_some_and(|v| !valid_duration(v)) {
            let bad = value.take().unwrap_or_default();
            w.push(format!("time.{key}=\"{bad}\" has an unsupported format — filter was skipped. Use a relative duration like \"7d\", \"2h\", \"1w\", or \"3m\"."))
        }
    }
    (Some(t), w)
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
        let root = std::env::temp_dir().join(format!("octocode-ast-lines-{}", std::process::id()));
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
        assert_eq!(error.code, "ast.execution.cancelled");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
