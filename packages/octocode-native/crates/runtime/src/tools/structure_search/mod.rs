//! Filesystem layout: `tree` outlines directories, `files` finds paths by name
//! or metadata. Walk-only by construction — this module never loads a grammar.
mod files;
#[cfg(test)]
mod tests;
mod tree;

/// Contract maximum of a structureSearch query field (both operations agree);
/// an undeclared bound stays open (validation enforces it).
fn structure_max(field: &str) -> u32 {
    crate::contracts::query_schema_number(
        crate::tools::id::ToolId::StructureSearch,
        None,
        field,
        "maximum",
    )
    .and_then(|maximum| u32::try_from(maximum).ok())
    .unwrap_or(u32::MAX)
}

/// Entries one walk may visit: the contract `limit` maximum.
fn max_walk() -> u32 {
    structure_max("limit")
}

use serde_json::{Value, json};

/// Continuations a listing row copies its query into: `next.nextPage` and
/// `next.expandLimit`.
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

/// A listed directory as the response renders it: the envelope anchors a
/// row's `dir` (relative to the walked root's parent; `""` is that parent)
/// on that parent, then names it workspace-relative (`.` for the workspace
/// itself), or absolute outside it.
fn rendered_dir(
    paths: &crate::policy::path::PathPolicy,
    root: &std::path::Path,
    dir: &str,
) -> String {
    let parent = root.parent().unwrap_or(root);
    let absolute = if dir.is_empty() {
        parent.to_path_buf()
    } else {
        parent.join(dir)
    };
    paths
        .workspace_relative(&absolute)
        .unwrap_or_else(|| absolute.to_string_lossy().into_owned())
}

use sha2::{Digest, Sha256};

pub use crate::contracts::tool_types::StructureSearchQuery;
use crate::policy::PolicyError;

#[derive(Clone, Debug, PartialEq)]
pub struct StructureError {
    pub code: String,
    pub message: String,
}

impl StructureError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<PolicyError> for StructureError {
    fn from(error: PolicyError) -> Self {
        // PolicyErrorCode serializes camelCase: `structure.policy.notFound`.
        let suffix = serde_json::to_value(error.code)
            .ok()
            .and_then(|code| code.as_str().map(str::to_owned))
            .unwrap_or_else(|| "io".to_owned());
        Self::new(format!("structure.policy.{suffix}"), error.message)
    }
}

pub type StructureResult = Result<Value, StructureError>;

fn cancelled(error: String) -> StructureError {
    StructureError::new("structure.execution.cancelled", error)
}

fn walk_error(error: impl ToString) -> StructureError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    let code = if message.starts_with("[structure.execution.cancelled]") {
        "structure.execution.cancelled"
    } else if lower.contains("no such file") || lower.contains("not found") {
        "structure.policy.notFound"
    } else if lower.contains("permission denied") {
        "structure.policy.permissionDenied"
    } else if lower.contains("invalid") && lower.contains("regex") {
        "structure.query.invalidPattern"
    } else if lower.starts_with("invalid ") {
        // The engine rejects a malformed filter value (`size`, `time`, ...)
        // before walking: caller input, like the typed time-filter checks.
        "invalidInput"
    } else {
        "structure.execution.failed"
    };
    StructureError::new(code, message)
}

fn allow_discovery(
    path: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::tools::cancel::CancellationCheck,
) -> Result<bool, String> {
    cancel
        .check()
        .map_err(|message| format!("[structure.execution.cancelled] {message}"))?;
    Ok(paths.permits_discovery(path))
}

fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn digest(value: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(value).unwrap_or_default());
    hex::encode(hasher.finalize())
}

/// Emitted when a page>1 request carries a snapshot that no longer matches the
/// digest of the query shape and ordered result set.
fn snapshot_changed(query: &impl serde::Serialize, snapshot: &str) -> Value {
    let mut restart = continuation(query, json!({"page":1}));
    if let Some(query) = restart["query"].as_object_mut() {
        query.remove("snapshot");
    }
    json!({
        "status":"error",
        "errorCode":"structure.snapshot.changed",
        "error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.",
        "snapshot":snapshot,
        "complete":false,
        "next":{"restart":restart}
    })
}

/// Copy the query with `changes` applied as a `structureSearch` continuation.
fn continuation(query: &impl serde::Serialize, changes: Value) -> Value {
    let mut query = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let (Some(to), Some(from)) = (query.as_object_mut(), changes.as_object()) {
        to.extend(from.clone())
    }
    json!({"tool":crate::tools::id::ToolId::StructureSearch.as_str(),"query":query,"confidence":"exact"})
}

/// An empty listing cannot prove absence in pruned `.gitignore`d entries.
/// Disclose that coverage gap and offer a retry including those entries.
fn note_ignored_empty(
    out: &mut Value,
    query: &impl serde::Serialize,
    ignored: usize,
    retry: Value,
    hint: String,
) {
    if ignored == 0 || out["status"] != "empty" {
        return;
    }
    let mut call = continuation(query, retry);
    if let Some(query) = call["query"].as_object_mut() {
        query.remove("snapshot");
    }
    out["hints"] = json!([hint]);
    out["next"]["includeIgnored"] = call;
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
