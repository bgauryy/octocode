//! Single-project coverage signals: TypeScript/JavaScript inferred projects
//! and, in `compile_database`, clangd without a compilation database.
//!
//! Without a `tsconfig.json`/`jsconfig.json` above the anchor file, tsserver
//! answers from an *inferred* project holding only the opened documents and
//! what they import. Incoming-direction questions (who references / calls /
//! implements this?) then silently miss every file that was never opened.
//! Such rows are flagged partial with reason `inferredProject`, a hint, and
//! a lexical `localSearch` fallback over the workspace.
//!
//! C-family rows record configuration files separately from an effective
//! compilation command. Missing configuration is a coverage warning; finding
//! a file does not verify the command or the server's indexed scope.

use super::failure::flag_partial;
use super::importers::{SCAN_CAPPED, SCAN_COMPLETE, SCAN_FAILED};
use super::paths::anchor_dir;
use super::scope::Scope;
use crate::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::workspace::typescript_project_dir;
use serde_json::Value;
use std::path::Path;

pub(super) const REASON: octocode_runtime_base::response::paging::PartialReason =
    octocode_runtime_base::response::paging::PartialReason::InferredProject;
/// Importer recovery failed a step, so some importers are unverified.
pub(in crate::lsp_search) const FAILED_REASON:
    octocode_runtime_base::response::paging::PartialReason =
    octocode_runtime_base::response::paging::PartialReason::ImporterScanFailed;

/// Operations whose answer depends on files the server has not opened.
pub(super) fn is_incoming(operation: &str) -> bool {
    matches!(
        operation,
        "references" | "callers" | "implementation" | "subtypes" | "workspaceSymbol"
    )
}

/// Flag an incoming-direction TS/JS row answered by an inferred project.
pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    anchor_path: &str,
    scope: &Scope,
) {
    use octocode_engine::lsp::family::{is_c_family, is_typescript_family};
    let typescript = is_typescript_family(language_id);
    if !(typescript || is_c_family(language_id))
        || !is_incoming(lsp_search_query::operation(query))
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    // Importer recovery opened and verified every file that mentions the
    // name, so an inferred project no longer hides importers. A window with
    // later windows left was flagged by `Importers::annotate`, which names
    // the window and its `next.nextImporterWindow`.
    // A failed scan was flagged with its cause by `Importers::annotate`.
    if matches!(
        row.pointer("/coverage/importerScan")
            .and_then(Value::as_str),
        Some(SCAN_COMPLETE | SCAN_CAPPED | SCAN_FAILED)
    ) || !typescript
        || typescript_project_dir(anchor_dir(Path::new(anchor_path))).is_some()
    {
        return;
    }
    // `inferredProject` is the whole statement: what it means (and the
    // tsconfig.json repair) is core's, said once in the output reference.
    flag_partial(row, query, REASON, None, scope);
}

pub(in crate::lsp_search) mod compile_database;

#[cfg(test)]
mod tests;
