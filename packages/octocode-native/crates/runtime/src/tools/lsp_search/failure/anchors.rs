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
    } else if lsp_search_query::operation(query) == "documentSymbols" {
        read["view"] = json!("symbols");
    }
    crate::response::result::attach_related(
        value,
        crate::response::result::Continuation::related(ToolId::LocalFetch, read).build(),
    );
}

pub(in super::super) fn anchor_recovery(
    value: &mut Value,
    query: &LspSearchQuery,
    source: Option<&str>,
) {
    let Some(name) = lsp_search_query::symbol_name(query) else {
        return;
    };
    if let Some(source) = source {
        keep_reads_that_find(value, query, name, source);
    }
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

/// The anchor's `localFetch` read stays only when it finds `name`
/// (case-sensitive, as the read asks). When the file holds `name` only in
/// another case, the error names the nearest such line to the anchor and
/// the read searches case-insensitively; with no occurrence at all, there
/// is nothing in the file to read, and only the workspace search remains.
fn keep_reads_that_find(value: &mut Value, query: &LspSearchQuery, name: &str, source: &str) {
    if name.is_empty() || source.contains(name) {
        return;
    }
    let mut read = None;
    if let Some(related) = value.get_mut("related").and_then(Value::as_array_mut) {
        related.retain(|call| {
            let reads_name = call["tool"] == "localFetch"
                && call["input"]["queries"][0]["matchString"] == json!([name]);
            if reads_name {
                read = Some(call["input"]["queries"][0].clone());
            }
            !reads_name
        });
    }
    let folded = name.to_lowercase();
    let anchor = lsp_search_query::anchor_line(query).unwrap_or(1);
    let Some(line) = source
        .lines()
        .zip(1u32..)
        .filter(|(text, _)| text.to_lowercase().contains(&folded))
        .map(|(_, number)| number)
        .min_by_key(|number| number.abs_diff(anchor))
    else {
        return;
    };
    if let Some(error) = value["error"].as_str() {
        value["error"] = json!(format!(
            "{error}; the file has '{name}' only in another case, nearest at line {line}"
        ));
    }
    if let Some(mut read) = read {
        read["caseMode"] = json!("insensitive");
        crate::response::result::attach_related(
            value,
            crate::response::result::Continuation::related(ToolId::LocalFetch, read).build(),
        );
    }
}
