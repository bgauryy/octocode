//! Filesystem layout: `tree` outlines directories, `files` finds paths by name
//! or metadata. Walk-only by construction — this module never loads a grammar.
mod files;
#[cfg(test)]
mod tests;
mod tree;

use serde_json::{Value, json};
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
    } else {
        "structure.execution.failed"
    };
    StructureError::new(code, message)
}

fn allow_discovery(
    path: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::tools::local_fetch::CancellationCheck,
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
fn snapshot_changed(snapshot: &str) -> Value {
    json!({
        "status":"error",
        "errorCode":"structure.snapshot.changed",
        "error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.",
        "snapshot":snapshot,
        "complete":false
    })
}

/// Copy the query with `changes` applied as a `structureSearch` continuation.
fn continuation(query: &impl serde::Serialize, changes: Value) -> Value {
    let mut query = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let (Some(to), Some(from)) = (query.as_object_mut(), changes.as_object()) {
        to.extend(from.clone())
    }
    json!({"tool":"structureSearch","query":query,"confidence":"exact"})
}

pub fn execute_structure(
    query: Value,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::tools::local_fetch::CancellationCheck,
) -> StructureResult {
    let query: StructureSearchQuery = serde_json::from_value(query)
        .map_err(|error| StructureError::new("structure.input.invalid", error.to_string()))?;
    match &query {
        StructureSearchQuery::Tree(query) => {
            tree::execute_tree(query, paths, security, cancellation)
        }
        StructureSearchQuery::Files(query) => {
            files::execute_files(query, paths, security, cancellation)
        }
    }
}
