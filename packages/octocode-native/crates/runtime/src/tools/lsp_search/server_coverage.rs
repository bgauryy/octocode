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
use serde_json::Value;
use std::path::Path;

mod gates;
mod includes;
use gates::*;
pub(super) use includes::package_includes;
use includes::{annotate_includes, record_text_coverage};

pub(super) const DYNAMIC_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::DynamicDispatch;
pub(super) const UNANSWERED_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ServerUnanswered;
/// The server's own status says it is not fully working.
const HEALTH_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ServerUnhealthy;
/// File-level: missing files import the name by a path the server did not
/// resolve to the anchor's module.
pub(super) const UNRESOLVED_IMPORT_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ImportUnresolved;
/// Why an `importUnresolved` row is partial; it reads the same on every page.
const UNRESOLVED_IMPORT_NOTE: &str = "Files that import the name are missing from the answer: the server did not resolve their import here.";
pub(super) const CFG_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::CfgGatedFiles;
pub(super) const CFG_NEGATED_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::CfgNegatedFeatures;
/// File-level: cleared when every file that uses the name is answered.
pub(super) const BUILD_SCRIPTS_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::BuildScriptsOff;
/// File-level, like [`BUILD_SCRIPTS_REASON`]; no build context recovers it.
pub(super) const OUTSIDE_INCLUDE_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::IncludeOutsidePackage;
/// File-level: the anchor file is a module of several crates.
pub(super) const SHARED_MODULE_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::SharedModuleFile;
/// The server's inference limit is stated once in the tool description.
const PYTHON_WARNING: &str = "Files that spell the name are missing from the answer.";
/// Largest file the gate scan reads; generated sources (a 2.4 MB
/// `tool_types.rs`) fit, so they never make every answer partial.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

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
    stopped: &dyn Fn() -> bool,
) {
    if !is_incoming(lsp_search_query::operation(query))
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
        || flagged_with(row, &FILE_LEVEL_REASONS)
    {
        return;
    }
    // What the server's project leaves out (cfg, the preprocessor) still spells
    // the name: said as a count, since a same-named symbol does too.
    if language_id == Some("rust") || octocode_engine::lsp::family::is_c_family(language_id) {
        record_text_coverage(row, query, scope);
    }
    if octocode_engine::lsp::family::is_c_family(language_id) {
        annotate_c_family_text_only(row, query, scope);
    }
    if scope.unbounded && lsp_search_query::op(query) != Op::WorkspaceSymbol && lists_no_use(row) {
        super::failure::flag_partial(row, query, UNANSWERED_REASON, Some(UNBOUNDED_NOTE), scope);
    }
    match language_id {
        // A workspace symbol is a declaration the server found: no use of it
        // is missing (`workspace_symbols` says what the protocol cannot prove).
        Some("python") if lsp_search_query::op(query) == Op::WorkspaceSymbol => {}
        Some("python") => annotate_python(row, query, scope),
        Some("rust") => {
            // A workspace symbol is a declaration the server found: no use
            // of it is missing.
            if lsp_search_query::op(query) != Op::WorkspaceSymbol {
                annotate_includes(
                    row,
                    query,
                    server.anchor,
                    scope,
                    rust.build_scripts,
                    stopped,
                );
            }
            annotate_rust(row, query, server.root, scope, rust.all_features, stopped);
        }
        // The rest have no signal of their own, and a server that loaded no
        // project answers empty just like one that found nothing.
        _ if lacks_signal(language_id) => annotate_unanswered(row, query, scope),
        _ => {}
    }
}

/// A server that says it is not working (rust-analyzer on a manifest it cannot
/// load) still answers, often empty, and its protocol calls that settled. On any
/// operation the row is partial, in the server's own words, and leads to the text.
pub(super) fn flag_server_health(
    row: &mut Value,
    query: &LspSearchQuery,
    health: Option<octocode_engine::lsp::client::ServerHealth>,
    scope: &Scope,
) {
    let Some(health) = health else {
        return;
    };
    if !row.is_object() || row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    let level = health.level.as_str();
    let note = match health.message {
        Some(message) => format!("Language server reports {level}: {message}"),
        None => format!("Language server reports {level}."),
    };
    flag_partial(row, query, HEALTH_REASON, Some(&note), scope);
}

/// Why an answer naming no use cannot speak for files beyond the server's root.
const UNBOUNDED_NOTE: &str = "No .git or workspace file says where this project ends, so only the server's root was searched; a module beside it may use this name. Pass workspaceRoot with the folder that holds them.";

/// A file that spells the name in code and is not in the answer may hold another
/// symbol or disabled code, or a use the index missed: a failed parse, a missing
/// generated header and a dependent call all read the same. Nothing here can tell
/// them apart, so the row says it is partial and leads to the text.
fn annotate_c_family_text_only(row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
    if lsp_search_query::op(query) == Op::WorkspaceSymbol {
        return;
    }
    let Some(files) = lsp_search_query::symbol_name(query)
        .filter(|name| !name.trim().is_empty())
        .and_then(|name| scope.text_only_files(name))
        .filter(|files| !files.is_empty())
    else {
        return;
    };
    let note = format!(
        "{} file(s) spell this name in code but are not in the answer: another symbol, code the preprocessor disabled, or a use the index missed.",
        files.len()
    );
    super::failure::flag_partial(row, query, UNANSWERED_REASON, Some(&note), scope);
}

/// Whether `row` names no use at all: empty, or rows that are all declarations.
/// A view without match rows (`view:"files"`) lists its files as uses.
pub(super) fn lists_no_use(row: &Value) -> bool {
    if row.get("status").and_then(Value::as_str) == Some("empty") {
        return true;
    }
    let Some(files) = row.get("files").and_then(Value::as_array) else {
        return false;
    };
    !files.is_empty()
        && files.iter().all(|file| {
            file["matches"].as_array().is_some_and(|found| {
                !found.is_empty() && found.iter().all(|hit| hit["role"] == "declaration")
            })
        })
}

/// Whether this language has no coverage signal of its own, so an answer naming
/// no use must be checked ([`annotate_unanswered`]).
pub(super) fn lacks_signal(language_id: Option<&str>) -> bool {
    use octocode_engine::lsp::family::{is_c_family, is_typescript_family};
    !(is_c_family(language_id)
        || is_typescript_family(language_id)
        || matches!(language_id, Some("python" | "rust")))
}

/// A server that lists no use while files it could not prove are another symbol
/// spell the name in code may not have loaded them: partial, with the lexical
/// lead. `page::run_target` has asked the server about each such file, and marked
/// those whose name resolves elsewhere answered, so a truly unused name stays
/// clean however common it is.
fn annotate_unanswered(row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
    if lsp_search_query::op(query) == Op::WorkspaceSymbol || !lists_no_use(row) {
        return;
    }
    let Some(files) = lsp_search_query::symbol_name(query)
        .filter(|name| !name.trim().is_empty())
        .and_then(|name| scope.text_only_files(name))
        .filter(|files| !files.is_empty())
    else {
        return;
    };
    let note = format!(
        "The server lists no use of this name, but {} other file(s) spell it in code: its project may not cover them.",
        files.len()
    );
    super::failure::flag_partial(row, query, UNANSWERED_REASON, Some(&note), scope);
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
const WORKSPACE_SYMBOL_CAPPED_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::WorkspaceSymbolCoverageUnknown;
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
    if let Some(lead) = super::failure::term_search_lead(name, &scope.root, &scope.include) {
        crate::response::result::attach_related(row, lead.build());
    }
    // A server cap this answer reached is a fact of this answer: a warning.
    let mut warnings = Vec::new();
    if let Some(limit) = capped_at {
        warnings.push(format!(
            "tsgo returns at most {limit} workspace symbols; this list is capped by the server: narrow symbolQuery, or route .ts to typescript-language-server in lsp-servers.json."
        ));
    }
    if !warnings.is_empty() {
        crate::response::paging::terminal_limit(row, WORKSPACE_SYMBOL_CAPPED_REASON, &warnings);
    }
    // This is an optional lead, moved to hints by continuation shaping;
    // it is not a page that recovers an exhaustive workspace result.
    if let Some(path) = anchor
        && let Some(outline) = lsp_search_query::document_lead(
            octocode_contract_types::LspDocumentOperation::DocumentSymbols,
            path,
            None,
        )
    {
        crate::response::result::attach_related(
            row,
            crate::response::result::Continuation::related_query(outline).build(),
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

fn flag_noted(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: crate::response::paging::PartialReason,
    note: String,
    scope: &Scope,
) {
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
fn lists_reason(row: &Value, reason: crate::response::paging::PartialReason) -> bool {
    let reason = reason.to_string();
    row.pointer("/coverage/reason").and_then(Value::as_str) == Some(reason.as_str())
        || row
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_some_and(|listed| listed.iter().any(|listed| *listed == reason.as_str()))
}

#[cfg(test)]
mod tests;
