//! Filesystem layout: `tree` outlines directories, `files` finds paths by name
//! or metadata. Walk-only by construction — this module never loads a grammar.
mod files;
mod memo;
mod output;
pub(crate) use output::Output;
#[cfg(test)]
mod tests;
mod tree;

/// Contract maximum of a structureSearch query field (both operations agree);
/// an undeclared bound stays open (validation enforces it).
fn structure_max(field: &str) -> u32 {
    let max =
        crate::contracts::query_schema_max(crate::tools::id::ToolId::StructureSearch, None, field);
    u32::try_from(max).unwrap_or(u32::MAX)
}

/// Entries one walk may visit: the contract `maxEntries` maximum.
fn max_walk() -> u32 {
    structure_max("maxEntries")
}

use crate::tools::display_name;
use serde_json::{Value, json};

/// Continuations a listing row copies its query into: `next.nextPage` and
/// `next.expandScan`.
const ROW_QUERY_COPIES: usize = 2;

/// Chars one default page's rows may take, so the page with the row around
/// it fits the response window. Measured on the query's snapshot identity:
/// a continuation that keeps its snapshot gets the same budget, so every
/// page of one snapshot cuts at the same rows.
fn page_budget(response_window: Option<usize>, identity: &Value) -> usize {
    crate::tools::stream_page::page_chars(
        response_window,
        crate::tools::stream_page::reserve_chars(identity, ROW_QUERY_COPIES),
    )
}

/// Serialized chars one row adds to a page: `entry` always, plus `header`
/// when the row opens a group: it starts the page, or `continues` is false
/// (it follows a row of another group). A page repeats the header of a group
/// it continues, so every page reads on its own.
#[derive(Clone, Copy, Debug)]
struct RowCost {
    entry: usize,
    header: usize,
    continues: bool,
}

/// Row ranges of each page: `page_size` rows each when the caller sets it,
/// else as many rows as fit `budget` serialized chars (at least one per
/// page). Costs come from snapshot-bound data, so every page of one snapshot
/// cuts at the same rows. An empty listing has no pages.
fn page_ranges(
    costs: &[RowCost],
    page_size: Option<usize>,
    budget: usize,
) -> Vec<std::ops::Range<usize>> {
    let total = costs.len();
    if total == 0 {
        return Vec::new();
    }
    if let Some(size) = page_size {
        let size = size.max(1);
        return (0..total)
            .step_by(size)
            .map(|start| start..start.saturating_add(size).min(total))
            .collect();
    }
    let mut pages = Vec::new();
    let mut start = 0;
    let mut used = 0usize;
    for (index, cost) in costs.iter().enumerate() {
        let opening = cost.entry.saturating_add(cost.header);
        let added = if cost.continues { cost.entry } else { opening };
        if index > start && used.saturating_add(added) > budget {
            pages.push(start..index);
            start = index;
            used = 0;
        }
        used = used.saturating_add(if index == start { opening } else { added });
    }
    pages.push(start..total);
    pages
}

pub use crate::contracts::tool_types::StructureSearchQuery;
use crate::tools::result::ToolError;

/// The policy error for `requested`; a missing path also leads to a tree of
/// its nearest existing parent, so the agent sees what is there instead.
/// The listed root once the call is live and the policy admits `requested`;
/// a missing root leads to its nearest admitted parent.
fn validated_root(
    requested: &str,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::tools::cancel::CancellationCheck,
) -> Result<crate::policy::path::ValidatedPath, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    paths
        .validate(requested)
        .map_err(|error| ToolError::root_policy(error, requested, paths, "pathValidationFailed"))
}

/// A file named as a tree root (`files` lists a file root as itself): say
/// so with its workspace-relative name and lead to its outline, the read an
/// agent wanted from it.
fn file_root(file: &std::path::Path, paths: &crate::policy::path::PathPolicy) -> ToolError {
    let name = paths
        .workspace_relative(file)
        .unwrap_or_else(|| file.to_string_lossy().into_owned());
    let mut out = ToolError::new(
        "notADirectory",
        format!("{name} is a file, not a directory; read it with localFetch or list its parent."),
    );
    let lead = crate::tools::result::Continuation::new(
        crate::tools::id::ToolId::LocalFetch,
        json!({"path": name, "minify": "symbols"}),
    )
    .build();
    out.next = Some(Box::new(json!({ "read": lead })));
    out
}

pub type StructureResult = Result<Value, ToolError>;

/// A failed walk of `requested`: a vanished root reads like any other
/// missing path.
fn walk_error(error: impl ToString, requested: &str) -> ToolError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    let code = if message.starts_with("[structure.execution.cancelled]") {
        "cancelled"
    } else if lower.contains("no such file") || lower.contains("not found") {
        return ToolError::new(
            crate::policy::PATH_NOT_FOUND,
            format!("Path does not exist: {requested}"),
        );
    } else if lower.contains("permission denied") {
        "permissionDenied"
    } else if lower.contains("invalid") && lower.contains("regex") {
        "invalidPattern"
    } else if lower.starts_with("invalid ") {
        // The engine rejects a malformed filter value (`size`, `time`, ...)
        // before walking: caller input, like the typed time-filter checks.
        "invalidInput"
    } else {
        "executionFailed"
    };
    ToolError::new(code, message)
}

fn allow_discovery(
    path: &std::path::Path,
    file_type: Option<std::fs::FileType>,
    discovery: &crate::policy::path::DiscoveryWalk<'_>,
    cancel: &dyn crate::tools::cancel::CancellationCheck,
) -> Result<bool, String> {
    cancel
        .check()
        .map_err(|message| format!("[structure.execution.cancelled] {message}"))?;
    Ok(discovery.permits_entry(path, file_type))
}

/// A continuation page whose walk no longer hashes to its `snapshot`: the
/// stale stored walk is dropped and the listing restarts from page 1.
fn restart_if_stale(
    query: &impl serde::Serialize,
    page: usize,
    snapshot: Option<&str>,
    current: &str,
) -> Option<Value> {
    if page <= 1 || snapshot == Some(current) {
        return None;
    }
    if let Some(stale) = snapshot {
        memo::evict(stale);
    }
    Some(crate::tools::result::stale_snapshot(continuation(
        query,
        json!({"page":1,"snapshot":null}),
    )))
}

/// Copy the query with `changes` applied as a `structureSearch` continuation;
/// a `null` change drops that field.
fn continuation(query: &impl serde::Serialize, changes: Value) -> Value {
    let mut query = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let (Some(to), Some(from)) = (query.as_object_mut(), changes.as_object()) {
        to.extend(from.clone());
        for (field, _) in from.iter().filter(|(_, value)| value.is_null()) {
            to.remove(field);
        }
    }
    crate::tools::result::Continuation::new(crate::tools::id::ToolId::StructureSearch, query)
        .confidence("exact")
        .build()
}

/// One level of the listed directory's tree: the subdirectories a listing
/// cut at a terminal limit can be re-listed under, each within the limits.
fn narrow_scope(query: &impl serde::Serialize) -> Value {
    let path = serde_json::to_value(query)
        .ok()
        .and_then(|query| query.get("path").cloned())
        .unwrap_or(Value::Null);
    crate::tools::result::Continuation::new(
        crate::tools::id::ToolId::StructureSearch,
        json!({"operation": "tree", "path": path, "maxDepth": 1}),
    )
    .why("Outline the subdirectories to list separately.")
    .build()
}

/// What a walk left out of a listing: `.gitignore`d entries, entries the
/// path policy withheld, dot entries skipped without `hidden`, and the
/// directories the default prune skipped (root-relative).
#[derive(Clone, Debug, Default)]
struct Uncovered {
    ignored: usize,
    /// Basenames of the `.gitignore`d directories (at most
    /// [`IGNORED_DIR_NAMES`]): one named like a default-pruned directory
    /// needs `defaultExcludes:false` too before its entries are walked.
    ignored_dirs: Vec<String>,
    withheld: crate::policy::discovery::Withheld,
    hidden: usize,
    pruned: Vec<String>,
}

/// Most `.gitignore`d directory names a walk keeps for its retry.
const IGNORED_DIR_NAMES: usize = 64;

/// Records each `.gitignore`d directory's basename, bounded, for
/// [`Uncovered::ignored_dirs`].
#[derive(Default)]
struct IgnoredDirs(std::sync::Mutex<Vec<String>>);

impl IgnoredDirs {
    fn record(&self, path: &std::path::Path) {
        if !path.is_dir() {
            return;
        }
        let mut names = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if names.len() < IGNORED_DIR_NAMES {
            names.push(display_name(path));
        }
    }
    fn into_names(self) -> Vec<String> {
        self.0
            .into_inner()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl Uncovered {
    /// The default-pruned directories as one clause: their count and each
    /// distinct name once.
    fn pruned_note(&self) -> Option<String> {
        if self.pruned.is_empty() {
            return None;
        }
        let mut names = self
            .pruned
            .iter()
            .map(|dir| dir.rsplit('/').next().unwrap_or(dir))
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        let count = self.pruned.len();
        Some(format!(
            "{count} default-excluded {} not walked: {}",
            if count == 1 { "dir" } else { "dirs" },
            names.join(", ")
        ))
    }

    /// An empty listing cannot prove absence in what the walk skipped: say
    /// so (the counts and names are in `summary`), and lead to the same
    /// listing with all of it included in one hop: a `.gitignore`d
    /// directory named like a default-pruned one (`target/`) needs both
    /// flags.
    fn note_empty(&self, out: &mut Value, query: &impl serde::Serialize) {
        if out["status"] != "empty" || (self.ignored == 0 && self.pruned.is_empty()) {
            return;
        }
        let pruned_names = crate::policy::prune::PruneMode::SyntaxVisible.directories(true);
        let ignored_pruned = self
            .ignored_dirs
            .iter()
            .any(|name| pruned_names.contains(name));
        let mut retry = json!({"page": 1, "snapshot": null});
        let (mut skipped, mut flags) = (Vec::new(), Vec::new());
        if self.ignored > 0 {
            retry["noIgnore"] = json!(true);
            skipped.push(".gitignore'd");
            flags.push("noIgnore:true");
        }
        if !self.pruned.is_empty() || ignored_pruned {
            retry["defaultExcludes"] = json!(false);
            skipped.push("default-excluded");
            flags.push("defaultExcludes:false");
        }
        out["hints"] = json!([format!(
            "Skipped {} entries may match; hints.includeIgnored sets {}.",
            skipped.join("/"),
            flags.join(", ")
        )]);
        out["next"]["includeIgnored"] = continuation(query, retry);
    }
}

/// Where a listing's rows were cut, for the continuation tail both
/// operations share.
struct Cut {
    page: usize,
    total_pages: usize,
    /// Rows in the listing after `maxEntries`.
    total: usize,
    requested: usize,
    /// Rows the walk returned before `maxEntries` cut them.
    available: usize,
    limit_cut: bool,
    scan_cut: bool,
    /// The walk stopped at the first match past the limit, so only a lower
    /// bound of the total is known.
    early_exit: bool,
    total_discovered: usize,
    /// Listing rows earlier windows listed (`scanOffset`).
    scan_offset: usize,
    /// Listing rows before this window's end: the next window's
    /// `scanOffset`.
    covered: usize,
    snapshot: String,
}

impl Cut {
    fn has_more(&self) -> bool {
        self.page < self.total_pages
    }
    fn can_expand(&self) -> bool {
        self.limit_cut && self.requested < max_walk() as usize
    }
    /// More pages exist, but the next page number is past the contract's
    /// `page` maximum.
    fn page_ceiling(&self) -> bool {
        self.has_more()
            && self.page >= crate::tools::id::query_limits::structure_search::PAGE_MAXIMUM
    }
    /// The walk stopped at the largest `maxEntries`: entries past it are
    /// on no page.
    fn walk_ceiling(&self) -> bool {
        (self.limit_cut || self.scan_cut) && !self.can_expand()
    }
    fn terminal(&self) -> bool {
        self.page_ceiling() || self.walk_ceiling()
    }
    fn out_of_range(&self) -> bool {
        self.total > 0 && self.page > self.total_pages
    }

    /// The page, scan-expansion and partial-coverage continuations.
    fn finish(&self, out: &mut Value, query: &impl serde::Serialize, warnings: &mut Vec<String>) {
        // Every listed row stays reachable: a walk ceiling ends the listing,
        // never the pages over the rows it did list.
        if self.has_more() && !self.page_ceiling() {
            out["next"]["nextPage"] = continuation(
                query,
                json!({"page": self.page + 1, "snapshot": self.snapshot}),
            );
        }
        if self.terminal() {
            let past = if self.page_ceiling() {
                format!(
                    "page {} is the last page a continuation may request; later rows of this listing",
                    self.page
                )
            } else if self.early_exit {
                format!(
                    "the walk stopped at maxEntries {}; entries past it",
                    self.requested
                )
            } else {
                format!(
                    "{} entries matched, past maxEntries {}; the rest",
                    self.total_discovered.max(self.available),
                    self.requested
                )
            };
            warnings.push(format!(
                "terminalLimit: {past} are on no page. List each subdirectory separately (hints.narrowScope outlines them) to reach them."
            ));
            out["next"]["narrowScope"] = narrow_scope(query);
        }
        // The widened walk resumes after this window: offered on its last
        // page, it lists only rows no page of this window showed.
        if self.can_expand() && !self.has_more() {
            let wider = self
                .requested
                .saturating_mul(2)
                .max(self.requested + 1)
                .min(max_walk() as usize);
            out["next"]["expandScan"] = continuation(
                query,
                json!({"maxEntries": wider, "scanOffset": self.covered, "page": 1, "snapshot": null}),
            );
        }
        if self.terminal() {
            out["terminalLimit"] = json!(true);
        }
        if self.limit_cut || self.scan_cut {
            let reasons = [(self.limit_cut, "maxEntries"), (self.scan_cut, "walkLimit")]
                .into_iter()
                .filter_map(|(cut, reason)| cut.then_some(reason))
                .collect::<Vec<_>>();
            out["truncated"] = json!(true);
            out["partialReasons"] = json!(reasons);
            if self.early_exit {
                out["atLeast"] = json!(self.scan_offset + self.total + 1);
            } else {
                out["totalAvailable"] = json!(self.total_discovered.max(self.available));
            }
        }
        if self.out_of_range() {
            warnings.push(format!(
                "page:{} is out of range (only {} page(s), {} entries). Use page:1..{}.",
                self.page, self.total_pages, self.total, self.total_pages
            ));
        }
    }
}

/// The directories that head a `{dir, …}` group.
fn group_dirs<'a>(dirs: impl Iterator<Item = &'a String>) -> std::collections::HashSet<String> {
    dirs.filter(|dir| !dir.is_empty()).cloned().collect()
}

/// The directory of a listed row as the response names it: relative to the
/// workspace root (SS2), `prefix` being the listed path's own spelling.
pub(super) fn group_dir(prefix: &str, dir: &str) -> String {
    match (prefix, dir) {
        (_, "") => prefix.to_owned(),
        (".", _) => dir.to_owned(),
        _ => format!("{prefix}/{dir}"),
    }
}

/// The listed directory (a file root: its directory) spelled like a row path: workspace-relative inside
/// the workspace, absolute outside it.
pub(super) fn listing_prefix(
    paths: &crate::policy::path::PathPolicy,
    root: &std::path::Path,
) -> String {
    // A file root lists itself under its directory.
    let dir = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root
    };
    paths
        .workspace_relative(dir)
        .unwrap_or_else(|| dir.to_string_lossy().into_owned())
}

/// One listing shape for `tree` and `files` (SS2/SS4): consecutive entries
/// of one directory become a `{dir, files}` group, `dir` workspace-relative
/// (the listed path's own entries included), so `dir + "/" + name` is a
/// path a local tool takes.
fn dir_groups<R>(
    rows: &[R],
    dir_entry: impl Fn(&R) -> (&String, &String),
    prefix: &str,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for row in rows {
        let (dir, entry) = dir_entry(row);
        let dir = group_dir(prefix, dir);
        match out.last_mut() {
            Some(group) if group.get("dir").and_then(Value::as_str) == Some(dir.as_str()) => {
                if let Some(items) = group["files"].as_array_mut() {
                    items.push(json!(entry));
                }
            }
            _ => out.push(json!({"dir": dir, "files": [entry]})),
        }
    }
    out
}

/// The walk for this page: a continuation reuses the walk its first page
/// stored when the listed entries are unchanged; otherwise walk now.
fn walked<T: std::any::Any + Send + Sync>(
    page: usize,
    snapshot: Option<&str>,
    policy: &str,
    walk: impl FnOnce() -> Result<T, ToolError>,
) -> Result<(std::sync::Arc<T>, bool), ToolError> {
    if page > 1
        && let Some(snapshot) = snapshot
        && let Some(stored) = memo::get::<T>(snapshot, policy)
    {
        return Ok((stored, true));
    }
    walk().map(|walk| (std::sync::Arc::new(walk), false))
}

/// Whether a filtered listing could have listed `path` had the policy
/// admitted it: a directory may hold matches; a file must pass the name
/// filters (include globs, extensions, basename regex).
struct FilterProbe {
    root: std::path::PathBuf,
    names: Option<globset::GlobSet>,
    paths: Option<globset::GlobSet>,
    extensions: Vec<String>,
    regex: Option<regex::Regex>,
}

impl FilterProbe {
    fn new(
        root: &std::path::Path,
        include: &[String],
        extensions: &[String],
        name_regex: Option<&str>,
    ) -> Self {
        let set = |globs: Vec<&String>| {
            if globs.is_empty() {
                return None;
            }
            let mut builder = globset::GlobSetBuilder::new();
            for glob in globs {
                // An uncompilable glob matches nothing in the walk either.
                if let Ok(glob) = globset::Glob::new(glob) {
                    builder.add(glob);
                }
            }
            builder.build().ok()
        };
        let (with_slash, bare): (Vec<_>, Vec<_>) = include.iter().partition(|g| g.contains('/'));
        Self {
            root: root.to_path_buf(),
            names: set(bare),
            paths: set(with_slash),
            extensions: extensions
                .iter()
                .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
                .collect(),
            regex: name_regex.and_then(|pattern| regex::Regex::new(pattern).ok()),
        }
    }

    fn could_list(&self, path: &std::path::Path) -> bool {
        if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
            return true;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let included = match (&self.names, &self.paths) {
            (None, None) => true,
            (names, paths) => {
                names.as_ref().is_some_and(|set| set.is_match(&name))
                    || paths.as_ref().is_some_and(|set| set.is_match(&relative))
            }
        };
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        included
            && (self.extensions.is_empty() || self.extensions.contains(&extension))
            && self
                .regex
                .as_ref()
                .is_none_or(|regex| regex.is_match(&name))
    }
}

/// Execute one typed row. The runtime parses the validated row with its
/// shared `parse_query`, so a shape mismatch has one code across tools.
pub fn execute_structure(
    query: &StructureSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::tools::cancel::CancellationCheck,
    response_window: Option<usize>,
) -> StructureResult {
    match query {
        StructureSearchQuery::Tree(query) => {
            tree::execute_tree(query, paths, security, cancellation, response_window)
        }
        StructureSearchQuery::Files(query) => {
            files::execute_files(query, paths, security, cancellation, response_window)
        }
    }
}
