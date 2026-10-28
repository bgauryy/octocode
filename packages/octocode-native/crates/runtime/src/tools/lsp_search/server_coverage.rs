//! Coverage signals where the protocol or server cannot establish
//! project-wide completeness.
//!
//! A Python server resolves a call only through a receiver whose type it
//! infers, so calls through untyped or dynamic receivers are missing:
//! reason `dynamicDispatch`, when files that spell the name are missing
//! from the answer. A missing file that imports the name holds an import
//! the server did not resolve to this declaration (a module path outside
//! its search roots): reason `importUnresolved`. In a package that `include!`s source rust-analyzer does
//! not load, uses through the types it defines are missing: reason
//! `buildScriptsOff` (build-script output, loaded by a rerun) or
//! `includeOutsidePackage` (a file outside the package), on the same
//! file-level evidence. rust-analyzer analyzes one feature set, so code
//! under disabled `cfg(feature)` gates is compiled out: reason
//! `cfgGatedFiles` when files the answer did not cover use the name in code
//! and carry such gates, with an `expandFeatures` rerun page. An all-features run compiles out
//! `cfg(not(feature …))` code instead: reason `cfgNegatedFeatures`. Every
//! such row gets the lexical `findText` lead. Gate scans cover the server's
//! Cargo workspace: code outside it is not compiled, so no gate there hides
//! a use.

use super::failure::{
    BUILD_SCRIPTS_COST, FILE_LEVEL_REASONS, TEXT_CHECK, flag_partial, flagged_with,
};
use super::inferred_project::is_incoming;
use super::scope::{Scope, imports_in_code};
use super::{LspSearchQuery, Op};
use crate::id::ToolId;
use crate::tools::lsp_search::lsp_search_query;
use serde_json::{Value, json};
use std::path::Path;

mod gates;
mod includes;
use gates::*;
pub(super) use includes::package_includes;
use includes::{annotate_includes, record_text_coverage};

pub(super) const DYNAMIC_REASON: &str = "dynamicDispatch";
/// File-level: missing files import the name by a path the server did not
/// resolve to the anchor's module.
pub(super) const UNRESOLVED_IMPORT_REASON: &str = "importUnresolved";
/// Why an `importUnresolved` row is partial; it reads the same on every page.
const UNRESOLVED_IMPORT_NOTE: &str = "Files that import the name are missing from the answer: the server did not resolve their import here.";
pub(super) const CFG_REASON: &str = "cfgGatedFiles";
pub(super) const CFG_NEGATED_REASON: &str = "cfgNegatedFeatures";
/// File-level: cleared when every file that uses the name is answered.
pub(super) const BUILD_SCRIPTS_REASON: &str = "buildScriptsOff";
/// File-level, like [`BUILD_SCRIPTS_REASON`]; no build context recovers it.
pub(super) const OUTSIDE_INCLUDE_REASON: &str = "includeOutsidePackage";
/// File-level: the anchor file is a module of several crates.
pub(super) const SHARED_MODULE_REASON: &str = "sharedModuleFile";
/// The server's inference limit is stated once in the tool description.
const PYTHON_WARNING: &str = "Files that spell the name are missing from the answer.";
/// Bound of the Rust gate scan; a larger workspace counts what it reached.
const MAX_SCANNED_FILES: usize = 20_000;
/// Largest file the gate scan reads; generated sources (a 2.4 MB
/// `tool_types.rs`) fit, so they never make every answer partial.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const GATE_SCAN_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

/// The lexical `findText` check, named by its search.
fn text_check() -> String {
    TEXT_CHECK.to_owned()
}

pub(super) fn annotate(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    server: Server<'_>,
    scope: &Scope,
    rust: super::receipt::RustAnalysis,
) {
    if !is_incoming(lsp_search_query::operation(query))
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
        || flagged_with(row, &FILE_LEVEL_REASONS)
    {
        return;
    }
    if language_id == Some("rust") {
        record_text_coverage(row, query, scope);
    }
    match language_id {
        Some("python") => annotate_python(row, query, scope),
        Some("rust") => {
            // A workspace symbol is a declaration the server found: no use
            // of it is missing.
            if lsp_search_query::op(query) != Op::WorkspaceSymbol {
                annotate_includes(row, query, server.anchor, scope, rust.build_scripts);
            }
            annotate_rust(row, query, server.root, scope, rust.all_features);
        }
        _ => {}
    }
}

/// Where the server answered: the anchor file and the server's workspace
/// root (for Rust, the Cargo workspace: the only code rust-analyzer
/// compiles, so the only code a cfg gate can hide).
#[derive(Clone, Copy)]
pub(super) struct Server<'a> {
    pub(super) anchor: &'a str,
    pub(super) root: &'a str,
}

/// Partial reason of a workspace-symbol answer the server's result limit cut.
const WORKSPACE_SYMBOL_CAPPED_REASON: &str = "workspaceSymbolCoverageUnknown";
/// Why no workspace-symbol answer proves completeness, and what checks the rest.
const WORKSPACE_SYMBOL_NOTE: &str = "workspace/symbol has no completeness marker: declarations the server did not index or return are unknown, and no page of this list completes it; confirm with a localSearch of the name.";

/// Workspace-symbol arrays have no total, cursor, or completeness marker.
/// Native pagination covers the returned array; it cannot establish that
/// the provider returned every matching declaration in its project. That
/// holds for every workspace/symbol answer, so it is a static notice, not
/// a partial flag: the row says it once and leads to the lexical search of
/// `name` over `scope`, the check no page of this answer can make.
/// `capped_at` is the server's own result limit when its answer reached it
/// (only tsgo has one): a fact of this answer, so the row is partial and a
/// warning says it outright.
pub(super) fn workspace_symbols(
    row: &mut Value,
    name: &str,
    scope: &Scope,
    anchor: Option<&str>,
    capped_at: Option<usize>,
) {
    if row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    crate::response::paging::push_notice(row, WORKSPACE_SYMBOL_NOTE);
    crate::response::result::attach_related(
        row,
        super::failure::term_search_lead(name, &scope.root, &scope.include).build(),
    );
    // A server cap this answer reached is a fact of this answer: a warning.
    let mut warnings = Vec::new();
    if let Some(limit) = capped_at {
        warnings.push(format!(
            "tsgo returns at most {limit} workspace symbols; this list is capped by the server: narrow symbolQuery, or route .ts to typescript-language-server in lsp-servers.json."
        ));
    }
    if !warnings.is_empty() {
        crate::response::paging::mark_partial(row, WORKSPACE_SYMBOL_CAPPED_REASON, &warnings);
    }
    if let Some(path) = anchor {
        // This is an optional lead, moved to hints by continuation shaping;
        // it is not a page that recovers an exhaustive workspace result.
        crate::response::result::attach_related(
            row,
            crate::response::result::Continuation::related(
                ToolId::LspSearch,
                json!({"operation": "documentSymbols", "path": path}),
            )
            .build(),
        );
    }
}

/// [`flag_partial`] with `note`, prose that reads the same on every page and
/// row of the answer: a static notice, said once, on the first page
/// ([`crate::response::paging::push_notice`]), and only when the row is flagged.
/// A Python row's misses by cause: files that import the name name an
/// unresolved import, the rest (or misses no text scan listed) the dynamic
/// dispatch the server cannot type.
fn annotate_python(row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
    let name = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty());
    let missed = name.and_then(|name| Some((name, scope.text_only_files(name)?)));
    let importers = missed.as_ref().map_or(0, |(name, files)| {
        files
            .iter()
            .filter(|file| imports_in_code(name, file))
            .count()
    });
    if importers > 0 {
        flag_noted(
            row,
            query,
            UNRESOLVED_IMPORT_REASON,
            UNRESOLVED_IMPORT_NOTE.into(),
            scope,
        );
    }
    if missed
        .as_ref()
        .is_none_or(|(_, files)| files.len() > importers || importers == 0)
    {
        flag_noted(row, query, DYNAMIC_REASON, PYTHON_WARNING.into(), scope);
    }
}

fn flag_noted(row: &mut Value, query: &LspSearchQuery, reason: &str, note: String, scope: &Scope) {
    flag_partial(row, query, reason, None, scope);
    if lists_reason(row, reason) {
        crate::response::paging::push_notice(row, note);
    }
}

/// Whether `query` continues an earlier page of its answer
/// ([`crate::response::paging::continues_a_page`]): a rerun lead is offered on
/// the first page only.
fn continues_a_page(query: &LspSearchQuery) -> bool {
    crate::response::paging::continues_a_page(ToolId::LspSearch, &lsp_search_query::to_row(query))
}

/// Whether `row` is flagged with `reason`.
fn lists_reason(row: &Value, reason: &str) -> bool {
    row.pointer("/coverage/reason").and_then(Value::as_str) == Some(reason)
        || row
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_some_and(|listed| listed.iter().any(|listed| listed == reason))
}

#[cfg(test)]
mod tests;
