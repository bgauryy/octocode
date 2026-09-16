use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine_core::types::{FileSystemEntry, FileSystemQueryOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const MAX_WALK: u32 = 10_000;
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeFilters {
    pub modified_within: Option<String>,
    pub modified_before: Option<String>,
    pub accessed_within: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SizeFilters {
    pub greater: Option<String>,
    pub less: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AstFilesQuery {
    #[serde(default = "files_op")]
    pub operation: String,
    pub path: String,
    pub max_depth: Option<u32>,
    pub min_depth: Option<u32>,
    pub names: Option<Vec<String>>,
    pub extensions: Option<Vec<String>>,
    pub path_pattern: Option<String>,
    pub path_regex: Option<String>,
    pub entry_type: Option<String>,
    pub empty: Option<bool>,
    pub time: Option<TimeFilters>,
    pub size: Option<SizeFilters>,
    pub permissions: Option<String>,
    pub access: Option<String>,
    pub exclude_dir: Option<Vec<String>>,
    pub limit: Option<u32>,
    #[serde(default = "one")]
    pub page: u32,
    #[serde(default = "hundred")]
    pub page_size: u32,
    pub detail: Option<String>,
    pub sort: Option<String>,
}
fn files_op() -> String {
    "files".into()
}
const fn one() -> u32 {
    1
}
const fn hundred() -> u32 {
    100
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
    q: &AstFilesQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    if q.operation != "files" {
        return Err(super::AstError::new(
            "ast.input.invalid",
            "operation must be files",
        ));
    }
    let validated = paths.validate(&q.path).map_err(super::AstError::from)?;
    let (time, mut warnings) = valid_time(q.time.clone());
    let access = q.access.as_deref();
    let native = octocode_engine_core::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            include_root: Some(true),
            recursive: Some(true),
            max_depth: q.max_depth,
            min_depth: q.min_depth,
            show_hidden: Some(true),
            names: q.names.clone(),
            extensions: q.extensions.clone(),
            path_pattern: q.path_pattern.clone(),
            regex: q.path_regex.clone(),
            entry_type: q.entry_type.clone(),
            empty: q.empty,
            modified_within: time.as_ref().and_then(|t| t.modified_within.clone()),
            modified_before: time.as_ref().and_then(|t| t.modified_before.clone()),
            accessed_within: time.as_ref().and_then(|t| t.accessed_within.clone()),
            size_greater: q.size.as_ref().and_then(|s| s.greater.clone()),
            size_less: q.size.as_ref().and_then(|s| s.less.clone()),
            permissions: q.permissions.clone(),
            executable: Some(access == Some("executable")),
            readable: Some(access == Some("readable")),
            writable: Some(access == Some("writable")),
            exclude_dir: q.exclude_dir.clone(),
            stop_at_limit: Some(true),
            limit: Some(MAX_WALK),
        },
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::native_error)?;
    cancel.check().map_err(super::cancelled)?;
    warnings.extend(walk_warnings(native.skipped, native.permission_denied));
    let full = q.detail.as_deref() == Some("full");
    // The immutable CLI's packaged filesystem primitive does not expose
    // modifiedMs. Keep the raw value for sorting, but do not synthesize a
    // public timestamp that the reference cannot return.
    let collect_modified = full
        || q.detail.as_deref() == Some("modified")
        || q.sort.as_deref().unwrap_or("modified") == "modified";
    let count_lines = (full || q.sort.as_deref() == Some("lines")) && native.entries.len() <= 2_000;
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
    sort_rows(
        &mut rows,
        q.sort.as_deref().unwrap_or("modified"),
        collect_modified,
    );
    let available = rows.len();
    let requested = q.limit.unwrap_or(MAX_WALK).min(MAX_WALK) as usize;
    rows.truncate(requested);
    let total = rows.len();
    let page_size = q.page_size.clamp(1, 100) as usize;
    let page = q.page.max(1) as usize;
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
    let mut out = json!({"path":super::display_name(&validated.canonical),"files":files,"pagination":{"currentPage":page,"totalPages":total_pages,"filesPerPage":page_size,"totalFiles":total,"hasMore":has_more}});
    if total == 0 {
        out["status"] = json!("empty")
    }
    if has_more && !terminal {
        out["pagination"]["nextPage"] = json!(page + 1);
        out["next"]["nextPage"] = continuation(q, json!({"page":page+1}))
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
    let mut output = json!({"path":path,"type":kind});
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
fn continuation(q: &AstFilesQuery, changes: Value) -> Value {
    let mut query = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(map) = query.as_object_mut() {
        map.retain(|_, v| !v.is_null());
    }
    if query.get("detail").is_none() {
        query["detail"] = json!("basic");
    }
    if query.get("sort").is_none() {
        query["sort"] = json!("modified");
    }
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
    let mut lines = 1_usize;
    loop {
        cancel.check().map_err(super::cancelled)?;
        match file.read(&mut buffer) {
            Ok(0) => return Ok(lines),
            Ok(bytes) => {
                lines += buffer[..bytes]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
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
fn valid_time(time: Option<TimeFilters>) -> (Option<TimeFilters>, Vec<String>) {
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
