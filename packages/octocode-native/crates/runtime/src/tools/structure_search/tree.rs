use crate::policy::prune::DefaultsFlag;
use crate::{
    policy::{path::PathPolicy, prune::PruneMode},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::types::FileSystemQueryOptions;
use serde_json::json;

use super::files::{format_size, walk_warnings};
use crate::contracts::tool_types::StructureSearchQueryTree;

const MAX_WALK: u32 = 10_000;

/// Engine-unit views over the generated `tree` query.
impl StructureSearchQueryTree {
    /// Levels below `path`; the engine counts immediate children as depth 1.
    fn walk_depth(&self) -> u32 {
        u32::try_from(self.max_depth.max(0))
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    }
    fn limit(&self) -> usize {
        self.limit
            .map_or(MAX_WALK as usize, |limit| {
                usize::try_from(limit.get()).unwrap_or(usize::MAX)
            })
            .min(MAX_WALK as usize)
    }
    fn page(&self) -> usize {
        usize::try_from(self.page.get()).unwrap_or(usize::MAX)
    }
    fn page_size(&self) -> usize {
        self.page_size
            .map_or(100, |size| {
                usize::try_from(size.get()).unwrap_or(usize::MAX)
            })
            .clamp(1, 100)
    }
}

/// Bounded directory outline. The engine walk visits children name-sorted,
/// depth-first, so walk order is already outline order: a directory row
/// precedes its contents and no re-sort is needed.
pub fn execute_tree(
    q: &StructureSearchQueryTree,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::StructureResult {
    cancel.check().map_err(super::cancelled)?;
    let validated = paths
        .validate(q.path.as_str())
        .map_err(super::StructureError::from)?;
    let native = octocode_engine::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            include_root: Some(false),
            recursive: Some(true),
            max_depth: Some(q.walk_depth()),
            show_hidden: Some(q.hidden.unwrap_or(false)),
            extensions: (!q.extensions.is_empty()).then(|| q.extensions.clone()),
            entry_type: q.entry_type.map(|kind| kind.to_string()),
            exclude_dir: Some(
                PruneMode::SyntaxVisible.directories(&q.exclude_dir, q.default_excludes.defaults()),
            ),
            stop_at_limit: Some(true),
            limit: Some(MAX_WALK),
            ..Default::default()
        },
        &|path| super::allow_discovery(path, paths, cancel),
    )
    .map_err(super::walk_error)?;
    cancel.check().map_err(super::cancelled)?;

    let mut files = 0_usize;
    let mut dirs = 0_usize;
    let mut bytes = 0_i64;
    let mut rows = native
        .entries
        .iter()
        .map(|entry| {
            let relative = security.sanitize_text(&entry.relative_path, None).content;
            match entry.entry_type.as_str() {
                "directory" => {
                    dirs += 1;
                    format!("{relative}/")
                }
                "symlink" => format!("{relative}@"),
                _ => {
                    files += 1;
                    let size = entry.size.unwrap_or(0);
                    bytes += size;
                    format!("{relative} ({})", format_size(size))
                }
            }
        })
        .collect::<Vec<_>>();
    let available = rows.len();
    let requested = q.limit();
    rows.truncate(requested);
    let total = rows.len();

    let snapshot = super::digest(&json!([
        q.path,
        q.max_depth,
        q.hidden,
        q.extensions,
        q.entry_type.map(|kind| kind.to_string()),
        q.exclude_dir,
        requested,
        rows
    ]));
    if q.page() > 1 && q.snapshot.as_deref().map(String::as_str) != Some(snapshot.as_str()) {
        return Ok(super::snapshot_changed(&snapshot));
    }
    let page_size = q.page_size();
    let page = q.page().max(1);
    let total_pages = total.div_ceil(page_size).max(1);
    let start = (page - 1).saturating_mul(page_size);
    let entries = rows
        .get(start..start.saturating_add(page_size).min(total))
        .unwrap_or(&[])
        .to_vec();
    let has_more = page < total_pages;
    let limit_cut = available > total;
    let scan_cut = native.was_capped;
    let can_expand = limit_cut && requested < MAX_WALK as usize;
    let terminal = (has_more && page >= 1000) || ((limit_cut || scan_cut) && !can_expand);

    let mut out = json!({
        "path": super::display_name(&validated.canonical),
        "entries": entries,
        "summary": format!("{available} entries ({files} files, {dirs} dirs, {})", format_size(bytes)),
        "snapshot": snapshot,
    });
    if total == 0 {
        out["status"] = json!("empty");
    }
    if total_pages > 1 || page > total_pages {
        out["pagination"] = json!({"currentPage":page,"totalPages":total_pages,"entriesPerPage":page_size,"totalEntries":total,"hasMore":has_more});
    }
    if has_more && !terminal {
        out["next"]["nextPage"] =
            super::continuation(q, json!({"page":page + 1,"snapshot":snapshot}));
    }
    if can_expand {
        out["next"]["expandLimit"] = super::continuation(
            q,
            json!({"limit":requested.saturating_mul(2).min(MAX_WALK as usize),"page":1}),
        );
    }
    if terminal {
        out["terminalLimit"] = json!(true);
    }
    if limit_cut || scan_cut {
        let reasons = [(limit_cut, "limit"), (scan_cut, "walkLimit")]
            .into_iter()
            .filter_map(|(cut, reason)| cut.then_some(reason))
            .collect::<Vec<_>>();
        out["truncated"] = json!(true);
        out["partialReasons"] = json!(reasons);
        out["totalAvailable"] = json!(usize::max(native.total_discovered as usize, available));
    }
    let mut warnings = walk_warnings(native.skipped, native.permission_denied);
    warnings.extend(native.warnings);
    if total > 0 && start >= total {
        warnings.push(format!(
            "page:{page} is out of range (only {total_pages} page(s), {total} entries). Use page:1..{total_pages}."
        ));
    }
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
    Ok(out)
}
