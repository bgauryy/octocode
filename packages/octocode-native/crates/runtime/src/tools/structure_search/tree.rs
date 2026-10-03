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
            .map_or(super::max_walk() as usize, |limit| {
                usize::try_from(limit.get()).unwrap_or(usize::MAX)
            })
            .min(super::max_walk() as usize)
    }
    fn page(&self) -> usize {
        usize::try_from(self.page.get()).unwrap_or(usize::MAX)
    }
    /// The caller's page size; `None` pages by the response budget.
    fn page_size(&self) -> Option<usize> {
        self.page_size.map(|size| {
            usize::try_from(size.get())
                .unwrap_or(usize::MAX)
                .clamp(1, super::structure_max("pageSize") as usize)
        })
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
    response_window: Option<usize>,
) -> super::StructureResult {
    cancel.check().map_err(super::cancelled)?;
    let validated = paths
        .validate(q.path.as_str())
        .map_err(super::StructureError::from)?;
    // Leave out what localSearch leaves out: `.gitignore`d entries (unless
    // noIgnore) and paths the sensitive-file policy denies. Denied entries
    // the listing would otherwise show are counted and reported, never
    // silently dropped.
    let gitignore = (!q.no_ignore.unwrap_or(false))
        .then(|| crate::policy::gitignore::GitignoreFilter::new(&validated.canonical));
    let show_hidden = q.hidden.unwrap_or(false);
    let withheld = std::sync::atomic::AtomicUsize::new(0);
    let ignored = std::sync::atomic::AtomicUsize::new(0);
    let hidden = std::sync::atomic::AtomicUsize::new(0);
    let pruned =
        PruneMode::SyntaxVisible.directories(&q.exclude_dir, q.default_excludes.defaults());
    let native = octocode_engine::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            include_root: Some(false),
            recursive: Some(true),
            max_depth: Some(q.walk_depth()),
            show_hidden: Some(show_hidden),
            names: (!q.names.is_empty()).then(|| q.names.clone()),
            extensions: (!q.extensions.is_empty()).then(|| q.extensions.clone()),
            entry_type: q.entry_type.map(|kind| kind.to_string()),
            exclude_dir: Some(pruned.clone()),
            stop_at_limit: Some(true),
            limit: Some(super::max_walk()),
            ..Default::default()
        },
        &|path| {
            // Dot entries out of view (no `hidden`) are neither listed nor
            // counted as withheld or ignored; the ones `hidden:true` would
            // walk (permitted, not ignored, not a pruned directory) are
            // counted as hidden.
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dot = name.starts_with('.');
            if gitignore
                .as_ref()
                .is_some_and(|filter| filter.is_ignored(path))
            {
                if show_hidden || !dot {
                    ignored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Ok(false);
            }
            let allowed = super::allow_discovery(path, paths, cancel)?;
            if !allowed && (show_hidden || !dot) && paths.is_sensitive(path) {
                withheld.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            if allowed && dot && !show_hidden && !(pruned.contains(&name) && path.is_dir()) {
                hidden.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(allowed)
        },
    )
    .map_err(super::walk_error)?;
    cancel.check().map_err(super::cancelled)?;
    let withheld = withheld.into_inner();
    let ignored = ignored.into_inner();
    let hidden = hidden.into_inner();

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

    let identity = json!([
        q.path,
        q.max_depth,
        q.hidden,
        q.no_ignore,
        q.names,
        q.extensions,
        q.entry_type.map(|kind| kind.to_string()),
        q.exclude_dir,
        requested,
        q.page_size(),
    ]);
    // The page cut is part of the snapshot: a continuation under a
    // different response window restarts instead of skipping entries.
    let budget = super::page_budget(response_window, &identity);
    let snapshot = super::digest(&json!([identity, budget, rows]));
    if q.page() > 1 && q.snapshot.as_deref().map(String::as_str) != Some(snapshot.as_str()) {
        return Ok(super::snapshot_changed(q, &snapshot));
    }
    let page_size = q.page_size();
    let page = q.page().max(1);
    // Entries are relative to `path` (no prefix, no groups); each costs its
    // quoted text.
    let costs = rows
        .iter()
        .map(|row| super::RowCost {
            entry: crate::tools::stream_page::json_chars(row) + 1,
            header: 0,
            continues: true,
        })
        .collect::<Vec<_>>();
    let pages = super::page_ranges(&costs, page_size, budget);
    let total_pages = pages.len().max(1);
    let entries = pages
        .get(page - 1)
        .map(|shown| rows[shown.clone()].to_vec())
        .unwrap_or_default();
    let has_more = page < total_pages;
    let limit_cut = available > total;
    let scan_cut = native.was_capped;
    let can_expand = limit_cut && requested < super::max_walk() as usize;
    let terminal = (has_more && page >= 1000) || ((limit_cut || scan_cut) && !can_expand);

    let mut out = json!({
        "path": super::display_name(&validated.canonical),
        "entries": entries,
        "summary": withheld_note(
            format!("{available} entries ({files} files, {dirs} dirs, {})", format_size(bytes)),
            ignored,
            withheld,
            hidden,
        ),
        "snapshot": snapshot,
    });
    if total == 0 {
        out["status"] = json!("empty");
    }
    if hidden > 0 {
        let mut call = super::continuation(q, json!({"hidden":true,"page":1}));
        if let Some(query) = call["query"].as_object_mut() {
            query.remove("snapshot");
        }
        out["next"]["includeHidden"] = call;
    }
    if total_pages > 1 || page > total_pages {
        out["pagination"] = json!({"currentPage":page,"totalPages":total_pages,"totalEntries":total,"hasMore":has_more});
        if let Some(size) = page_size {
            out["pagination"]["entriesPerPage"] = json!(size);
        }
    }
    if has_more && !terminal {
        out["next"]["nextPage"] =
            super::continuation(q, json!({"page":page + 1,"snapshot":snapshot}));
    }
    if can_expand {
        out["next"]["expandLimit"] = super::continuation(
            q,
            json!({"limit":requested.saturating_mul(2).min(super::max_walk() as usize),"page":1}),
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
    if total > 0 && page > total_pages {
        warnings.push(format!(
            "page:{page} is out of range (only {total_pages} page(s), {total} entries). Use page:1..{total_pages}."
        ));
    }
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
    super::note_ignored_empty(
        &mut out,
        q,
        ignored,
        json!({"noIgnore":true,"page":1}),
        format!(
            "The walk pruned {ignored} .gitignore'd entries; whether they match these filters is unproven. next.includeIgnored retries with noIgnore:true."
        ),
    );
    Ok(out)
}

/// The summary, plus how many entries `.gitignore` hid, how many the
/// sensitive-file policy withheld (credentials such as `.env.production` or
/// `.npmrc`), and how many dot entries were skipped without `hidden`: their
/// names stay out of the listing, but their absence is not silent.
fn withheld_note(mut summary: String, ignored: usize, withheld: usize, hidden: usize) -> String {
    let entries = |count: usize| if count == 1 { "entry" } else { "entries" };
    if ignored > 0 {
        summary.push_str(&format!(
            "; {ignored} {} hidden by .gitignore (noIgnore:true lists them)",
            entries(ignored)
        ));
    }
    if withheld > 0 {
        summary.push_str(&format!(
            "; {withheld} sensitive {} withheld by path policy",
            entries(withheld)
        ));
    }
    if hidden > 0 {
        summary.push_str(&format!(
            "; {hidden} dot {} skipped (hidden:true includes them)",
            entries(hidden)
        ));
    }
    summary
}
