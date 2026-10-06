use crate::policy::prune::DefaultsFlag;
use crate::{
    policy::{path::PathPolicy, prune::PruneMode},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::types::FileSystemQueryOptions;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use super::files::{format_size, walk_warnings};
use crate::contracts::tool_types::StructureSearchQueryTree;

/// Engine-unit views over the generated `tree` query.
impl StructureSearchQueryTree {
    /// Levels below `path` (1 = its children), as the engine counts them.
    /// Omitted, a listing shows `path`'s children; an `include` filter
    /// searches every level, like ghStructure.
    fn walk_depth(&self) -> u32 {
        self.max_depth.map_or_else(
            || {
                if self.include.is_empty() {
                    1
                } else {
                    super::structure_max("maxDepth")
                }
            },
            |depth| u32::try_from(depth.get()).unwrap_or(u32::MAX),
        )
    }
    fn max_entries(&self) -> usize {
        self.max_entries
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
    /// A name or type filter narrows the listing, so what it skipped may
    /// hold the entries asked for.
    fn filtered(&self) -> bool {
        !self.include.is_empty() || !self.extensions.is_empty() || self.entry_type.is_some()
    }
}

/// One outline row: its directory relative to `path` (`""` is `path`) and
/// its entry there: the name, then `/` (directory), `@` (symlink) or
/// ` (<size>)` (file).
struct TreeRow {
    dir: String,
    entry: String,
    /// The walked entry, for the continuation pages' change check.
    source: std::path::PathBuf,
}

/// A tree walk: its rows in listing order (cut to `maxEntries`) and what
/// the walk left out.
struct TreeWalk {
    rows: Vec<TreeRow>,
    available: usize,
    total_discovered: usize,
    was_capped: bool,
    files: usize,
    dirs: usize,
    bytes: i64,
    uncovered: super::Uncovered,
    warnings: Vec<String>,
}

impl super::memo::Listed for TreeWalk {
    fn sources(&self) -> Vec<&std::path::Path> {
        self.rows.iter().map(|row| row.source.as_path()).collect()
    }
}

/// Page layout tag folded into the snapshot: rows grouped by directory.
const TREE_LAYOUT: &str = "treeDirGroups";

/// Bounded directory outline. The engine walk visits children name-sorted,
/// depth-first; `limit` cuts that walk, then rows regroup by directory.
pub fn execute_tree(
    q: &StructureSearchQueryTree,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> super::StructureResult {
    let validated = super::validated_root(q.path.as_str(), paths, cancel)?;
    if validated.canonical.is_file() {
        return Err(super::file_root(&validated.canonical, paths));
    }
    let requested = q.max_entries();
    // The canonical root, not the spelling: a continuation may name the same
    // directory relative to the workspace.
    let identity = json!([
        validated.canonical.to_string_lossy(),
        q.walk_depth(),
        q.hidden,
        q.no_ignore,
        q.include,
        q.extensions,
        q.entry_type.map(|kind| kind.to_string()),
        q.exclude,
        q.default_excludes,
        requested,
        q.page_size(),
    ]);
    let policy = paths.identity();
    let snapshot_in = q.snapshot.as_deref().map(String::as_str);
    let (walk, stored) = super::walked(q.page(), snapshot_in, &policy, || {
        walk_tree(q, &validated.canonical, paths, security, cancel, requested)
    })?;
    // The page cut is part of the snapshot: a continuation under a
    // different response window restarts instead of skipping entries.
    let budget = super::page_budget(response_window, &identity);
    let snapshot = crate::digest::json_sha256(&json!([
        identity,
        budget,
        TREE_LAYOUT,
        walk.rows
            .iter()
            .map(|row| (&row.dir, &row.entry))
            .collect::<Vec<_>>()
    ]));
    if let Some(restart) = super::restart_if_stale(q, q.page(), snapshot_in, &snapshot) {
        return Ok(restart);
    }
    let pages = super::page_ranges(&tree_costs(&walk.rows), q.page_size(), budget);
    if !stored && pages.len() > 1 {
        super::memo::put(snapshot.clone(), policy, &validated.canonical, &walk);
    }
    Ok(tree_page(
        q,
        &walk,
        &pages,
        snapshot,
        &validated.canonical,
        requested,
    ))
}

/// Walk the tree once: rows in listing order, cut to `requested`, with
/// directories that have their own group not also listed bare.
fn walk_tree(
    q: &StructureSearchQueryTree,
    root: &std::path::Path,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    requested: usize,
) -> Result<TreeWalk, super::StructureError> {
    // Leave out what localSearch leaves out: `.gitignore`d entries (unless
    // noIgnore) and paths the sensitive-file policy denies. Denied entries
    // the listing would otherwise show are counted and reported, never
    // silently dropped.
    let gitignore = (!q.no_ignore.unwrap_or(false))
        .then(|| crate::policy::gitignore::GitignoreFilter::new(root));
    let show_hidden = q.hidden.unwrap_or(false);
    let include = crate::policy::include::include_globs(&q.include);
    let probe = super::FilterProbe::new(root, &include, &q.extensions, None);
    let withheld = std::sync::Mutex::new(crate::policy::discovery::Withheld::default());
    let (ignored, hidden) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let pruned = PruneMode::SyntaxVisible.directories(q.default_excludes.defaults());
    let discovery = paths.discovery_walk();
    let native = octocode_engine::portable::query_file_system_filtered(
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            include_root: Some(false),
            recursive: Some(true),
            max_depth: Some(q.walk_depth()),
            show_hidden: Some(show_hidden),
            names: (!include.is_empty()).then(|| include.clone()),
            extensions: (!q.extensions.is_empty()).then(|| q.extensions.clone()),
            entry_type: q.entry_type.map(|kind| kind.to_string()),
            exclude_dir: Some(pruned.clone()),
            exclude: (!q.exclude.is_empty()).then(|| q.exclude.clone()),
            stop_at_limit: Some(true),
            limit: Some(super::max_walk()),
            ..Default::default()
        },
        &|path| {
            // Dot entries out of view (no `hidden`) are neither listed nor
            // counted as withheld or ignored; the ones `hidden:true` would
            // walk (permitted, not ignored, not a pruned directory) are
            // counted as hidden.
            let name = crate::tools::display_name(path);
            let dot = name.starts_with('.');
            if gitignore
                .as_ref()
                .is_some_and(|filter| filter.is_ignored(path))
            {
                if show_hidden || !dot {
                    ignored.fetch_add(1, Relaxed);
                }
                return Ok(false);
            }
            let allowed = super::allow_discovery(path, &discovery, cancel)?;
            if !allowed
                && (show_hidden || !dot)
                && crate::policy::discovery::is_sensitive_path(path)
                && probe.could_list(path)
            {
                withheld
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .record(path, path.is_dir());
            }
            if allowed && dot && !show_hidden && !(pruned.contains(&name) && path.is_dir()) {
                hidden.fetch_add(1, Relaxed);
            }
            Ok(allowed)
        },
    )
    .map_err(|error| super::walk_error(error, q.path.as_str()))?;
    cancel.check().map_err(super::cancelled)?;
    let (mut rows, files, dirs, bytes) = tree_rows(&native.entries, security);
    let available = rows.len();
    rows.truncate(requested);
    // One group per directory: `path`'s own entries first, then each
    // directory in walk order (component-wise path order), entries in walk
    // order within it. The sort is stable.
    rows.sort_by(|a, b| std::path::Path::new(&a.dir).cmp(std::path::Path::new(&b.dir)));
    drop_grouped_dirs(&mut rows);
    let mut warnings = walk_warnings(native.skipped, native.permission_denied);
    warnings.extend(native.warnings);
    Ok(TreeWalk {
        rows,
        available,
        total_discovered: native.total_discovered as usize,
        was_capped: native.was_capped,
        files,
        dirs,
        bytes,
        uncovered: super::Uncovered {
            ignored: ignored.into_inner(),
            withheld: withheld
                .into_inner()
                .unwrap_or_else(|error| error.into_inner()),
            hidden: hidden.into_inner(),
            pruned: native.pruned_dirs,
        },
        warnings,
    })
}

/// Outline rows in walk order, with the file and directory counts and the
/// files' total size.
fn tree_rows(
    entries: &[octocode_engine::types::FileSystemEntry],
    security: &ContentSecurity,
) -> (Vec<TreeRow>, usize, usize, i64) {
    let (mut files, mut dirs, mut bytes) = (0_usize, 0_usize, 0_i64);
    let rows = entries
        .iter()
        .map(|entry| {
            let relative = security.sanitize_text(&entry.relative_path, None).content;
            let (dir, name) = relative.rsplit_once('/').unwrap_or(("", &relative));
            let text = match entry.entry_type.as_str() {
                "directory" => {
                    dirs += 1;
                    format!("{name}/")
                }
                "symlink" => format!("{name}@"),
                _ => {
                    files += 1;
                    let size = entry.size.unwrap_or(0);
                    bytes += size;
                    format!("{name} ({})", format_size(size))
                }
            };
            TreeRow {
                dir: dir.to_owned(),
                entry: text,
                source: std::path::PathBuf::from(&entry.path),
            }
        })
        .collect::<Vec<_>>();
    (rows, files, dirs, bytes)
}

/// A directory with its own `{dir, entries}` group is named by that group;
/// listing it bare (`"name/"`) under its parent too would show it twice,
/// even when the two land on different pages.
fn drop_grouped_dirs(rows: &mut Vec<TreeRow>) {
    let grouped = super::group_dirs(rows.iter().map(|row| &row.dir));
    rows.retain(|row| {
        let Some(name) = row.entry.strip_suffix('/') else {
            return true;
        };
        let path = if row.dir.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", row.dir)
        };
        !grouped.contains(&path)
    });
}

/// A row costs its quoted entry, plus its group's `{dir, entries}` header
/// whenever it opens one (`path`'s own entries have none).
fn tree_costs(rows: &[TreeRow]) -> Vec<super::RowCost> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| super::RowCost {
            entry: crate::tools::stream_page::json_chars(&row.entry) + 1,
            header: if row.dir.is_empty() {
                0
            } else {
                crate::tools::stream_page::json_chars(&json!({"dir":row.dir,"entries":[]})) + 1
            },
            continues: index > 0 && rows[index - 1].dir == row.dir,
        })
        .collect()
}

/// The response for this page of a walk.
fn tree_page(
    q: &StructureSearchQueryTree,
    walk: &TreeWalk,
    pages: &[std::ops::Range<usize>],
    snapshot: String,
    root: &std::path::Path,
    requested: usize,
) -> Value {
    let page = q.page().max(1);
    let total = walk.rows.len();
    let cut = super::Cut {
        page,
        total_pages: pages.len().max(1),
        total,
        requested,
        available: walk.available,
        limit_cut: walk.available > requested,
        scan_cut: walk.was_capped,
        early_exit: false,
        total_discovered: walk.total_discovered,
        snapshot: snapshot.clone(),
    };
    let entries = pages
        .get(page - 1)
        .map(|shown| {
            super::dir_groups(
                &walk.rows[shown.clone()],
                |row| (&row.dir, &row.entry),
                "entries",
            )
        })
        .unwrap_or_default();
    let mut out = json!({
        "path": super::display_name(root),
        "entries": entries,
        "snapshot": snapshot,
    });
    if total == 0 {
        out["status"] = json!("empty");
    }
    // The walk's summary and its retries describe the whole listing: page 1
    // states them once.
    if page == 1 {
        out["summary"] = json!(summary(walk));
        // Skipped dot entries matter when the listing is filtered or empty;
        // an unfiltered outline names their count in the summary.
        if walk.uncovered.hidden > 0 && (total == 0 || q.filtered()) {
            out["next"]["includeHidden"] =
                super::continuation(q, json!({"hidden":true,"page":1,"snapshot":null}));
        }
    }
    if cut.total_pages > 1 || page > cut.total_pages {
        out["pagination"] = json!({"currentPage":page,"totalPages":cut.total_pages,"totalItems":total,"hasMore":cut.has_more()});
        if let Some(size) = q.page_size() {
            out["pagination"]["pageSize"] = json!(size);
        }
    }
    let mut warnings = walk.warnings.clone();
    cut.finish(&mut out, q, &mut warnings);
    warnings.extend(walk.uncovered.withheld.notice());
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
    walk.uncovered.note_empty(&mut out, q);
    out
}

/// The walk's counts, plus how many entries `.gitignore` hid (the path
/// policy's withheld entries are a warning on every page), how many dot entries were skipped without `hidden`, and which
/// directories the default prune skipped: their names stay out of the
/// listing, but their absence is not silent.
fn summary(walk: &TreeWalk) -> String {
    let uncovered = &walk.uncovered;
    let entries = |count: usize| if count == 1 { "entry" } else { "entries" };
    let mut summary = format!(
        "{} entries ({} files, {} dirs, {})",
        walk.available,
        walk.files,
        walk.dirs,
        format_size(walk.bytes)
    );
    if uncovered.ignored > 0 {
        summary.push_str(&format!(
            "; {} {} hidden by .gitignore (noIgnore:true lists them)",
            uncovered.ignored,
            entries(uncovered.ignored)
        ));
    }
    if uncovered.hidden > 0 {
        summary.push_str(&format!(
            "; {} dot {} skipped (hidden:true lists them)",
            uncovered.hidden,
            entries(uncovered.hidden)
        ));
    }
    if let Some(note) = uncovered.pruned_note() {
        summary.push_str(&format!("; {note}"));
    }
    summary
}
