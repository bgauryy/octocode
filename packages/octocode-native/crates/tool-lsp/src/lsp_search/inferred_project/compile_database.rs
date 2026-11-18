//! C-family coverage: configuration files clangd reads, recorded apart from
//! an effective compilation command.

use super::super::analysis_context;
use super::super::failure::flag_partial;
use super::super::paths::anchor_dir;
use super::super::scope::Scope;
use super::is_incoming;
use crate::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::clangd_database::{clangd_config_sources, database_search};
use octocode_engine::lsp::types::LspLanguageServerConfig;
use serde_json::{Value, json};
use std::path::Path;

/// Coverage reason of a C/C++ row answered without a compilation database.
pub(in crate::lsp_search) const NO_COMPILE_DATABASE_REASON:
    octocode_runtime_base::response::paging::PartialReason =
    octocode_runtime_base::response::paging::PartialReason::NoCompileDatabase;

/// Record discovered configuration separately from the effective command.
/// File presence cannot establish command validity or indexed coverage.
/// The files checked follow clangd's own lookup for the server's `config`.
pub(in crate::lsp_search) fn annotate_compile_database(
    row: &mut Value,
    query: &LspSearchQuery,
    config: &LspLanguageServerConfig,
    anchor_path: &str,
    scope: &Scope,
) {
    if !octocode_engine::lsp::family::is_c_family(config.language_id.as_deref())
        || !row.is_object()
        || row.get("status").and_then(Value::as_str) == Some("error")
    {
        return;
    }
    let search = database_search(config);
    let sources: Vec<String> = clangd_config_sources(anchor_dir(Path::new(anchor_path)), &search)
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let found = !sources.is_empty();
    analysis_context::context(row)["compilation"] = json!({
        "configuration": if found { "found" } else { "notFound" },
        "command": "unverified",
        "sources": sources,
    });
    if found {
        return;
    }
    // `noCompileDatabase` is the whole statement: what it means and its
    // repair (generate the database) are core's, said once.
    let empty = row.get("status").and_then(Value::as_str) == Some("empty")
        || row.pointer("/status").and_then(Value::as_str) == Some("empty");
    if empty || crosses_units(lsp_search_query::operation(query)) {
        flag_partial(row, query, NO_COMPILE_DATABASE_REASON, None, scope);
    }
}

/// The compilation `sources` as the read policy spells a path: relative to
/// the workspace, `~/…` under the home directory, else the file name; never
/// an absolute home path.
pub(in crate::lsp_search) fn redact_sources(
    row: &mut Value,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
) {
    let Some(sources) = row
        .pointer_mut("/lsp/analysisContext/compilation/sources")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for source in sources {
        if let Some(path) = source.as_str() {
            *source = json!(paths.redact(path));
        }
    }
}

/// Whether an answer of `operation` can lie in another translation unit,
/// which only clangd's index reaches: every incoming operation, and a
/// definition (without the index it is the declaration the anchor's unit
/// includes, not the body another unit compiles).
fn crosses_units(operation: &str) -> bool {
    operation == "definition" || is_incoming(operation)
}

#[cfg(test)]
mod tests;
