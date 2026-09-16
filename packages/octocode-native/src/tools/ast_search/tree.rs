use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine_core::types::{FileSystemEntry, FileSystemQueryOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AstTreeQuery {
    #[serde(default = "tree_op")]
    pub operation: String,
    #[serde(default = "fs_kind")]
    pub tree_kind: String,
    pub path: String,
    pub max_depth: Option<u32>,
    pub hidden: Option<bool>,
    pub extensions: Option<Vec<String>>,
    pub entry_type: Option<String>,
    pub exclude_dir: Option<Vec<String>>,
    pub limit: Option<u32>,
    #[serde(default = "one")]
    pub page: u32,
    #[serde(default = "hundred")]
    pub page_size: u32,
    pub detail: Option<String>,
    pub sort: Option<String>,
    pub reverse: Option<bool>,
    pub name_pattern: Option<String>,
}
fn tree_op() -> String {
    "tree".into()
}
fn fs_kind() -> String {
    "filesystem".into()
}
const fn one() -> u32 {
    1
}
const fn hundred() -> u32 {
    100
}
struct Entry {
    value: Value,
    name: String,
    kind: String,
    size: i64,
    modified: f64,
    extension: String,
    relative: String,
}

pub fn execute_tree(
    q: &AstTreeQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    if q.operation != "tree" || q.tree_kind != "filesystem" {
        return Err(super::AstError::new(
            "ast.input.invalid",
            "operation must be tree with treeKind filesystem",
        ));
    }
    let validated = paths.validate(&q.path).map_err(super::AstError::from)?;
    let recursive = q.max_depth.unwrap_or(0) > 0;
    let max_depth = q.max_depth.unwrap_or(1);
    let native_names = q
        .name_pattern
        .as_ref()
        .filter(|p| !p.contains('['))
        .map(|p| {
            vec![if p.contains('*') || p.contains('?') {
                p.clone()
            } else {
                format!("*{p}*")
            }]
        });
    let native = octocode_engine_core::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            recursive: Some(recursive),
            include_root: Some(false),
            show_hidden: Some(q.hidden.unwrap_or(false)),
            max_depth: Some(max_depth),
            names: native_names,
            extensions: q.extensions.clone(),
            entry_type: q.entry_type.clone(),
            exclude_dir: q.exclude_dir.clone(),
            limit: Some(10_000),
            stop_at_limit: Some(true),
            ..Default::default()
        },
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::native_error)?;
    cancel.check().map_err(super::cancelled)?;
    let rich = matches!(q.detail.as_deref(), Some("full" | "modified"));
    let mut entries = native
        .entries
        .iter()
        .filter(|e| post_name(e, q.name_pattern.as_deref()))
        .map(|e| convert(e, &validated.canonical, security, rich))
        .collect::<Vec<_>>();
    let reverse = q.reverse.unwrap_or(false);
    entries.sort_by(|a, b| {
        let c = match q.sort.as_deref().unwrap_or("name") {
            "size" => a.size.cmp(&b.size),
            "time" => a.modified.total_cmp(&b.modified),
            "extension" => a.extension.cmp(&b.extension),
            _ => a.name.cmp(&b.name),
        };
        if reverse { c.reverse() } else { c }
    });
    let available = entries.len();
    if let Some(limit) = q.limit {
        entries.truncate(limit as usize)
    }
    let total = entries.len();
    let size = q.page_size.clamp(1, 100) as usize;
    let pages = total.div_ceil(size).max(1);
    let requested = q.page.max(1) as usize;
    let page = requested.min(pages);
    let out_of_range = requested > pages;
    let start = (page - 1) * size;
    let slice = &entries[start..(start + size).min(total)];
    let has_more = page < pages;
    let mut out = if rich {
        json!({"path":super::display_name(&validated.canonical),"entries":slice.iter().map(|e|e.value.clone()).collect::<Vec<_>>()})
    } else {
        let files = slice
            .iter()
            .filter(|e| e.kind == "file")
            .map(|e| format!("{} ({})", e.relative, format_size(e.size)))
            .collect::<Vec<_>>();
        let folders = slice
            .iter()
            .filter(|e| e.kind == "directory")
            .map(|e| e.relative.clone())
            .collect::<Vec<_>>();
        let links = slice
            .iter()
            .filter(|e| e.kind == "symlink")
            .map(|e| e.relative.clone())
            .collect::<Vec<_>>();
        let mut v = json!({"path":super::display_name(&validated.canonical)});
        if !files.is_empty() {
            v["files"] = json!(files)
        }
        if !folders.is_empty() {
            v["folders"] = json!(folders)
        }
        if !links.is_empty() {
            v["links"] = json!(links)
        }
        v
    };
    let files = entries.iter().filter(|e| e.kind == "file").count();
    let dirs = entries.iter().filter(|e| e.kind == "directory").count();
    let bytes = entries
        .iter()
        .filter(|e| e.kind == "file")
        .map(|e| e.size)
        .sum::<i64>();
    out["summary"] = json!(format!(
        "{total} entries ({files} files, {dirs} dirs, {})",
        format_size(bytes)
    ));
    if has_more || pages > 1 || out_of_range {
        out["pagination"] = json!({"currentPage":page,"totalPages":pages,"entriesPerPage":size,"totalEntries":total,"hasMore":has_more});
    }
    let limit_cut = available > total;
    let scan_cut = native.was_capped;
    let terminal = (has_more && page >= 1000)
        || scan_cut
        || (limit_cut && q.limit.unwrap_or(10_000) >= 10_000);
    if total == 0 {
        out["status"] = json!("empty")
    }
    if has_more && !terminal {
        out["pagination"]["nextPage"] = json!(page + 1);
        out["next"]["nextPage"] = continuation(q, json!({"page":page+1}))
    }
    if limit_cut && q.limit.unwrap_or(10_000) < 10_000 {
        out["next"]["expandLimit"] = continuation(
            q,
            json!({"limit":q.limit.unwrap_or(0).saturating_mul(2).max(q.limit.unwrap_or(0)+1).min(10_000),"page":1}),
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
        out["totalAvailable"] = json!(usize::max(native.total_discovered as usize, available))
    }
    let mut warnings = walk_warnings(native.skipped, native.permission_denied);
    warnings.extend(native.warnings);
    if out_of_range {
        out["pagination"]["outOfRange"] = json!(true);
        warnings.push(format!("page:{} is out of range (only {pages} page(s), {total} total entries) — returned page {page} instead.",q.page))
    }
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings)
    }
    Ok(out)
}
fn convert(
    e: &FileSystemEntry,
    root: &std::path::Path,
    security: &ContentSecurity,
    _rich: bool,
) -> Entry {
    let relative = security.sanitize_text(&e.relative_path, None).content;
    let kind = match e.entry_type.as_str() {
        "directory" => "directory",
        "symlink" => "symlink",
        _ => "file",
    }
    .to_owned();
    let size = e.size.unwrap_or(0);
    let modified = e.modified_ms.unwrap_or(0.);
    let root_name = super::display_name(root);
    let display_path = if e.relative_path.is_empty() {
        root_name
    } else {
        format!("{root_name}/{}", e.relative_path)
    };
    let mut value = json!({"type":match kind.as_str(){"directory"=>"dir","symlink"=>"link",_=>"file"},"path":security.sanitize_text(&display_path,None).content});
    if e.depth > 0 {
        value["depth"] = json!(e.depth)
    }
    if kind == "file" && e.size.is_some() {
        value["size"] = json!(format_size(size))
    }
    Entry {
        value,
        name: relative.clone(),
        kind,
        size,
        modified,
        extension: e.extension.clone().unwrap_or_default(),
        relative,
    }
}
fn post_name(e: &FileSystemEntry, p: Option<&str>) -> bool {
    let Some(p) = p else { return true };
    if p.contains('[') {
        e.name.contains(p)
    } else {
        true
    }
}
fn continuation(q: &AstTreeQuery, c: Value) -> Value {
    let mut v = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(map) = v.as_object_mut() {
        map.retain(|_, value| !value.is_null());
    }
    if v.get("detail").is_none() {
        v["detail"] = json!("basic");
    }
    if v.get("sort").is_none() {
        v["sort"] = json!("name");
    }
    if let (Some(a), Some(b)) = (v.as_object_mut(), c.as_object()) {
        a.extend(b.clone())
    }
    json!({"tool":"astSearch","query":v,"confidence":"exact"})
}
fn format_size(n: i64) -> String {
    super::files::format_size(n)
}
fn walk_warnings(s: u32, d: u32) -> Vec<String> {
    super::files::walk_warnings(s, d)
}
