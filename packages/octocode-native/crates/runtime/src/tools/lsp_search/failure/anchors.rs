//! Path and anchor recovery: directory anchors, nearby names, order hints
//! and the recovery `next` call.
use super::*;

/// A request whose path did not validate, as a row where its lead says
/// what to do (a directory anchor, a missing path), else the failure.
pub(in super::super) fn path_failure(
    query: &LspSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    failure: LspFailure,
) -> Result<Value, LspFailure> {
    if let Some(row) = directory_anchor(query, paths) {
        return Ok(row);
    }
    if failure.code != ErrorCode::PathNotFound {
        return Err(failure);
    }
    let mut row = json!({
        "status": "error",
        "errorCode": failure.code,
        "error": failure.message,
        "lsp": { "serverAvailable": false },
    });
    let requested = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    if let Some(next) = crate::response::result::missing_path_lead(paths, &requested) {
        crate::response::result::attach_calls(&mut row, next);
    }
    Ok(row)
}

/// The `notAFile` row of a request whose file anchor names a directory,
/// leading into it: a whole-word search for the named symbol, else the
/// directory's file listing. `None` when the path is no authorized directory.
pub(in super::super) fn directory_anchor(
    query: &LspSearchQuery,
    paths: &crate::policy::path::PathPolicy,
) -> Option<Value> {
    let decoded = super::super::render::decode_uri_path(lsp_search_query::path(query)?).ok()?;
    let directory = paths
        .validate(&decoded)
        .ok()
        .filter(|valid| valid.canonical.is_dir())?
        .canonical;
    let name = paths
        .workspace_relative(&directory)
        .unwrap_or_else(|| directory.to_string_lossy().into_owned());
    let mut row = json!({
        "status": "error",
        "errorCode": ErrorCode::NotAFile,
        "error": format!("{name} is a directory, not a file."),
        "lsp": { "serverAvailable": false },
    });
    match lsp_search_query::symbol_name(query).filter(|symbol| !symbol.trim().is_empty()) {
        Some(symbol) => {
            crate::response::result::attach_related(
                &mut row,
                text_search_lead(symbol, &name, &[], None).build(),
            );
        }
        None => {
            crate::response::result::attach_related(
                &mut row,
                crate::response::result::Continuation::related(
                    ToolId::StructureSearch,
                    json!({"operation": "files", "path": name}),
                )
                .build(),
            );
        }
    }
    Some(row)
}

/// `next.read` recovery: the symbol's match windows when a name anchored
/// the request (not the whole file), otherwise the file's default view.
pub(in super::super) fn attach_recovery_next(value: &mut Value, query: &LspSearchQuery) {
    let path = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    let symbol = lsp_search_query::symbol_name(query)
        .or_else(|| lsp_search_query::symbol_query(query))
        .filter(|name| !name.trim().is_empty());
    // A directory (a workspaceSymbol scope) is searched, not fetched.
    if let Some(symbol) = symbol.filter(|_| std::path::Path::new(&path).is_dir()) {
        crate::response::result::attach_related(
            value,
            symbol_search_lead(query, symbol, &path).build(),
        );
        return;
    }
    let mut read = json!({ "path": path });
    if let Some(symbol) = symbol {
        read["matchString"] = json!([symbol]);
        read["caseMode"] = json!("sensitive");
        read["contextLines"] = json!({"before":3,"after":3});
    }
    crate::response::result::attach_related(
        value,
        crate::response::result::Continuation::related(ToolId::LocalFetch, read).build(),
    );
}

pub(in super::super) fn anchor_recovery(
    value: &mut Value,
    query: &LspSearchQuery,
    _source: Option<&str>,
) {
    let Some(name) = lsp_search_query::symbol_name(query) else {
        return;
    };
    let mut search = json!({"operation": "workspaceSymbol", "symbolQuery": name});
    let asked = lsp_search_query::to_row(query);
    for key in ["path", "workspaceRoot", "rustContext"] {
        if let Some(value) = asked.get(key).filter(|value| !value.is_null()) {
            search[key] = value.clone();
        }
    }
    crate::response::result::attach_related(
        value,
        crate::response::result::Continuation::related(ToolId::LspSearch, search).build(),
    );
}
