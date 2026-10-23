//! Filesystem layout: `tree` outlines directories, `files` finds paths by name
//! or metadata. Walk-only by construction — this module never loads a grammar.
mod cut;
mod files;
mod listing;
mod memo;
mod output;
mod probe;
use octocode_contract_types::ErrorCode;
pub(crate) use output::Output;
mod resume;
mod rows;
#[cfg(test)]
mod tests;
mod tree;
mod walk_policy;
use crate::response::result::Confidence;
use cut::*;
use listing::Listing;
use probe::*;
use rows::{EntryKind, Row};
use walk_policy::*;

pub use octocode_contract_types::StructureSearchQuery;

/// Entries one walk may visit: the contract `maxEntries` maximum.
fn max_walk() -> u32 {
    u32_of_usize(crate::contracts::query_limits::structure_search::MAX_ENTRIES_MAXIMUM)
}

/// Wall time one listing walk may spend: the budget every unbounded local
/// scan shares. A filter that matches little makes the walk visit the whole
/// tree, which no entry count bounds; past this budget the walk stops at its
/// next entry and the page says where ([`Cut::time_cut`]).
const WALK_BUDGET: std::time::Duration = octocode_engine::scan_budget::SCAN_TIME_BUDGET;

use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::tools::display_name;
use crate::tools::empty_walk::{Cause, Hits, Tagged, Walk, diagnose_shown};
use serde_json::{Value, json};

/// The policy error for `requested`; a missing path also leads to a tree of
/// its nearest existing parent, so the agent sees what is there instead.
/// The listed root once the call is live and the policy admits `requested`;
/// a missing root leads to its nearest admitted parent.
fn validated_root(
    requested: &str,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<crate::policy::path::ValidatedPath, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    paths
        .validate(requested)
        .map_err(|error| ToolError::root_policy(error, requested, paths))
}

/// A file named as a tree root (`files` lists a file root as itself): say
/// so with its workspace-relative name and lead to its outline, the read an
/// agent wanted from it.
fn file_root(file: &std::path::Path, paths: &crate::policy::path::PathPolicy) -> ToolError {
    let name = paths
        .workspace_relative(file)
        .unwrap_or_else(|| file.to_string_lossy().into_owned());
    let mut out = ToolError::new(
        ErrorCode::NotADirectory,
        format!("{name} is a file, not a directory; read it with localFetch or list its parent."),
    );
    let lead = crate::response::result::Continuation::new(
        crate::id::ToolId::LocalFetch,
        json!({"path": name, "minify": "symbols"}),
    )
    .build();
    out.next = Some(Box::new(json!({ "read": lead })));
    out
}

pub type StructureResult = Result<Value, ToolError>;

/// A failed walk of `requested`, coded by the engine's typed kind: a
/// vanished root reads like any other missing path, and a walk the policy
/// callback stopped because the call was cancelled says so.
fn walk_error(
    error: octocode_engine::error::Error,
    requested: &str,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> ToolError {
    use octocode_engine::error::ErrorKind;
    if let Err(message) = cancel.check() {
        return ToolError::cancelled(message);
    }
    let code = match error.kind() {
        ErrorKind::NotFound => {
            return ToolError::new(
                ErrorCode::PathNotFound,
                format!("Path does not exist: {requested}"),
            );
        }
        ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        ErrorKind::InvalidPattern => ErrorCode::InvalidPattern,
        ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
        // A malformed filter value (`size`, ...) is rejected before the
        // walk: caller input, like the typed time-filter checks.
        ErrorKind::InvalidInput => ErrorCode::InvalidInput,
        _ => ErrorCode::ExecutionFailed,
    };
    ToolError::new(code, error.to_string())
}

/// The row kind a listing shows for an entry: anything that is neither a
/// directory nor a link lists as a file.
/// Whether a withheld entry is worth a warning: the repository's own `.git`
/// is withheld from every walk of a repository, so naming it says nothing
/// about this call.
fn reports_withheld(path: &std::path::Path) -> bool {
    display_name(path) != ".git"
}

/// A continuation page whose walk no longer hashes to its `snapshot`: the
/// stale stored walk is dropped and the listing restarts from its first
/// page and window. A page without a snapshot pins nothing: it pages the
/// current walk.
fn restart_if_stale(
    query: &impl serde::Serialize,
    page: usize,
    snapshot: Option<&str>,
    current: &str,
) -> Option<Value> {
    let stale = snapshot.filter(|snapshot| page > 1 && *snapshot != current)?;
    memo::evict(stale);
    let row = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    Some(crate::response::result::stale_snapshot(
        crate::response::result::restart_query(crate::id::ToolId::StructureSearch, &row),
    ))
}

/// Copy the query with `changes` applied as a `structureSearch` continuation;
/// a `null` change drops that field.
fn continuation(query: &impl serde::Serialize, changes: Value) -> Value {
    let query = crate::response::result::with_changes(
        serde_json::to_value(query).unwrap_or_else(|_| json!({})),
        &changes,
    );
    crate::response::result::Continuation::new(crate::id::ToolId::StructureSearch, query)
        .confidence(Confidence::Exact)
        .build()
}

/// One level of the listed directory's tree: the subdirectories a listing
/// cut at a terminal limit can be re-listed under, each within the limits.
fn narrow_scope(query: &impl serde::Serialize) -> Value {
    let path = serde_json::to_value(query)
        .ok()
        .and_then(|query| query.get("path").cloned())
        .unwrap_or(Value::Null);
    crate::response::result::Continuation::new(
        crate::id::ToolId::StructureSearch,
        json!({"operation": "tree", "path": path, "maxDepth": 1}),
    )
    .build()
}

/// What bounds a walk: a listing walk by time ([`WALK_BUDGET`]), a probe
/// walk by its own entry cap and deadline.
#[derive(Clone, Copy)]
enum Walking<'a> {
    Listing(std::time::Duration),
    Probe(&'a ProbeBound),
}

impl<'a> Walking<'a> {
    fn probe(self) -> Option<&'a ProbeBound> {
        match self {
            Self::Probe(bound) => Some(bound),
            Self::Listing(_) => None,
        }
    }

    fn budget(self) -> Option<std::time::Duration> {
        match self {
            Self::Listing(budget) => Some(budget),
            Self::Probe(_) => None,
        }
    }
}

/// How a listing walk keeps and uses its cursor ([`resume`]): the walk's
/// identity and the path policy it ran under, and its root.
struct Resumes<'a> {
    identity: Value,
    paths: &'a crate::policy::path::PathPolicy,
    root: &'a std::path::Path,
}

impl Resumes<'_> {
    /// The engine options for a listing walk of `walking`: the cursor of the
    /// window it resumes at `walk_offset` (when one is kept and nothing it
    /// depends on changed), and a checkpoint at the window's `requested`-th
    /// row, where a window cut at its rows ends.
    fn options(
        &self,
        walking: Walking<'_>,
        walk_offset: u32,
        requested: usize,
    ) -> (
        Option<octocode_engine::search::fs_resume::WalkCursor>,
        Option<u32>,
    ) {
        if walking.budget().is_none() {
            return (None, None);
        }
        let resume = (walk_offset > 0)
            .then(|| resume::take(&self.identity, &self.paths.identity(), walk_offset))
            .flatten();
        (resume, Some(u32_of_usize(requested)))
    }

    /// Keep where the walk stands at the end of its window, for the window
    /// that resumes there.
    fn keep(
        &self,
        native: &mut octocode_engine::types::FileSystemQueryResult,
        walking: Walking<'_>,
    ) {
        if walking.budget().is_none() || native.time_cut.is_none() {
            return;
        }
        if let Some(cursor) = native
            .cursor
            .take()
            .filter(|cursor| cursor.position() == native.walk_position)
        {
            resume::keep(
                &self.identity,
                &self.paths.identity(),
                self.paths,
                self.root,
                cursor,
            );
        }
    }
}

/// The directory of a listed row as the response names it: relative to the
/// workspace root, `prefix` being the listed path's own spelling.
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

/// The walk for this page: a continuation reuses the walk its first page
/// stored when the listed entries are unchanged; otherwise walk now.
fn walked(
    page: usize,
    snapshot: Option<&str>,
    policy: &str,
    walk: impl FnOnce() -> Result<Walked, ToolError>,
) -> Result<(std::sync::Arc<Walked>, bool), ToolError> {
    if page > 1
        && let Some(snapshot) = snapshot
        && let Some(stored) = memo::get::<Walked>(snapshot, policy)
    {
        return Ok((stored, true));
    }
    walk().map(|walk| (std::sync::Arc::new(walk), false))
}

/// A walk cut by time lists at most `requested` rows, the first ones in walk
/// order: the window ends at the last of them (`walk_position`), and the walk
/// resumed there reaches the rows after it. Cut anywhere else (the best rows
/// by a sort of a walk that timing cut short) the window could not tell the
/// resume which rows it dropped, and two windows would overlap or leave a gap.
/// Entries are in walk order; the root entry (position 0) is no row. A
/// listing that is not in walk order (`in_walk_order`) cannot skip the
/// `scan_offset` rows an earlier window listed over a walk cut short: the
/// window lists nothing.
fn end_window(
    native: &mut octocode_engine::types::FileSystemQueryResult,
    requested: usize,
    scan_offset: usize,
    in_walk_order: bool,
) -> TimeWindow {
    if native.time_cut.is_none() {
        return TimeWindow::Whole;
    }
    if scan_offset > 0 && !in_walk_order {
        native.entries.clear();
        return TimeWindow::SortedPastScanOffset;
    }
    let mut rows = native
        .entries
        .iter()
        .map(|entry| entry.walk_position)
        .filter(|position| *position > 0);
    // Fewer rows than `requested`, or exactly that many: nothing to end early.
    let Some(last) = rows.nth(requested.saturating_sub(1)) else {
        return TimeWindow::Whole;
    };
    if rows.next().is_none() {
        return TimeWindow::Whole;
    }
    // The window ends at `last`: the entries and the counts through it, the
    // rest is the resume's.
    native.end_at(last);
    TimeWindow::EndedAtRows
}

/// One listing walk: its rows in listing order (cut to `maxEntries`) and
/// what the walk saw beside them.
struct Walked {
    rows: Vec<Row>,
    meta: WalkMeta,
    /// The digest of the rows a snapshot binds, computed once per walk: a
    /// stored walk's continuation pages reuse it instead of hashing every
    /// row again.
    rows_digest: std::sync::OnceLock<String>,
}

/// What a listing walk saw beside its rows.
struct WalkMeta {
    /// Listing rows before this window's end (`scanOffset` plus the rows
    /// this window took): where `next.continueWalk` resumes.
    covered: usize,
    available: usize,
    total_discovered: usize,
    was_capped: bool,
    /// The walk ran out of time before this root-relative entry.
    time_cut: Option<String>,
    time_window: TimeWindow,
    /// Walk entries the window covers, `walkOffset` ones included.
    walk_position: usize,
    /// Files, directories and file bytes of this window's rows, counted
    /// before directories with their own group fold into its header.
    files: usize,
    dirs: usize,
    bytes: u64,
    uncovered: Uncovered,
    /// The listing is ranked by something other than walk order (`files`
    /// sorted by size, lines, name or time): a walk cut at its ceiling ranks
    /// only the entries it walked first.
    ranked: bool,
}

impl memo::Listed for Walked {
    fn sources(&self) -> Vec<memo::Seen<'_>> {
        self.rows.iter().map(Row::seen).collect()
    }
}

impl Walked {
    /// The walk's `rows` (listing order) cut to `requested`, less the
    /// `scan_offset` rows earlier windows listed (listing order is stable,
    /// so those are the ones they showed), with what the walk left out.
    fn finish(
        native: octocode_engine::types::FileSystemQueryResult,
        mut rows: Vec<Row>,
        requested: usize,
        scan_offset: usize,
        left: WalkLeft<'_>,
        security: &crate::security::ContentSecurity,
    ) -> Self {
        let available = rows.len();
        rows.truncate(requested);
        let offset = scan_offset.min(rows.len());
        rows.drain(..offset);
        // A non-directory row is a file; bytes are regular files' (a
        // symlink's own length is no content).
        let (mut files, mut dirs, mut bytes) = (0, 0, 0);
        for row in &rows {
            match row.kind {
                EntryKind::Directory => dirs += 1,
                EntryKind::Symlink => files += 1,
                EntryKind::File => {
                    files += 1;
                    bytes += row.seen.0;
                }
            }
        }
        let meta = WalkMeta {
            covered: offset + rows.len(),
            available,
            total_discovered: native.total_discovered as usize,
            was_capped: native.was_capped,
            time_cut: native
                .time_cut
                .map(|at| security.sanitize_text(&at, None).content),
            time_window: TimeWindow::Whole,
            walk_position: native.walk_position as usize,
            files,
            dirs,
            bytes,
            uncovered: left.into_uncovered(
                native.ignored,
                native.walk_position,
                (native.skipped, native.permission_denied),
            ),
            ranked: false,
        };
        Self {
            rows,
            meta,
            rows_digest: std::sync::OnceLock::new(),
        }
    }

    /// [`Self::rows_digest`]: the digest of each row's [`Sourced::shown`],
    /// in listing order.
    fn rows_digest(&self) -> &str {
        self.rows_digest.get_or_init(|| {
            crate::digest::json_sha256(&self.rows.iter().map(Row::shown).collect::<Vec<_>>())
        })
    }

    /// Where this page cuts the listing. An early-exit walk that found one
    /// more match than the limit has more; only a full walk knows the total.
    fn cut(
        &self,
        page: usize,
        pages: usize,
        requested: usize,
        early_exit: bool,
        (scan_offset, walk_offset): (usize, u32),
        snapshot: String,
    ) -> Cut {
        let meta = &self.meta;
        let (limit_cut, scan_cut) = if early_exit {
            (meta.was_capped, false)
        } else {
            (meta.available > requested, meta.was_capped)
        };
        Cut {
            page,
            total_pages: pages.max(1),
            total: self.rows.len(),
            requested,
            available: meta.available,
            limit_cut,
            scan_cut,
            time_cut: meta.time_cut.clone(),
            time_window: meta.time_window,
            walk_position: meta.walk_position,
            walk_offset: walk_offset as usize,
            early_exit,
            total_discovered: meta.total_discovered,
            scan_offset,
            covered: meta.covered,
            ranked: meta.ranked,
            snapshot,
        }
    }
}

impl WalkMeta {
    /// The page's warnings: `cut`'s own (`warnings`). The query's notes
    /// read the same for the whole result: they are notices, said once
    /// ([`crate::response::paging::push_notice`]).
    fn warn(&self, out: &mut Value, query_notes: Vec<String>, cut: Vec<String>) {
        if !cut.is_empty() {
            out["warnings"] = json!(cut);
        }
        for note in query_notes {
            crate::response::paging::push_notice(out, note);
        }
    }
}

/// Execute one typed row. The runtime parses the validated row with its
/// shared `parse_query`, so a shape mismatch has one code across tools.
pub fn execute_structure(
    query: &StructureSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::execution::cancel::CancellationCheck,
    response_window: Option<usize>,
) -> StructureResult {
    execute_structure_within(
        query,
        paths,
        security,
        cancellation,
        response_window,
        WALK_BUDGET,
    )
}

/// [`execute_structure`] with the listing walk's time budget named.
fn execute_structure_within(
    query: &StructureSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::execution::cancel::CancellationCheck,
    response_window: Option<usize>,
    budget: std::time::Duration,
) -> StructureResult {
    match query {
        StructureSearchQuery::Tree(query) => listing::execute_listing(
            query,
            paths,
            security,
            cancellation,
            response_window,
            budget,
        ),
        StructureSearchQuery::Files(query) => listing::execute_listing(
            query,
            paths,
            security,
            cancellation,
            response_window,
            budget,
        ),
    }
}
