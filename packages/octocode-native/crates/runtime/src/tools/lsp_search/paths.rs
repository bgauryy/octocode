//! Admitting the query's path and workspace root: policy checks, directory
//! roots and the caller's spelling of the root.
use super::*;

pub(super) fn set_path(query: &mut LspSearchQuery, path: String) {
    match query {
        LspSearchQuery::AnchoredLocations(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredReferences(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredCallers(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredHierarchy(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredSubtypes(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredHover(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::Document(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::WorkspacePath(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::WorkspaceRoot(query) => query.path = Some(path),
    }
}

pub(super) fn set_workspace_root(query: &mut LspSearchQuery, root: String) {
    match query {
        LspSearchQuery::WorkspaceRoot(query) => {
            if let Ok(root) = root.try_into() {
                query.workspace_root = root;
            }
        }
        LspSearchQuery::AnchoredLocations(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredReferences(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredCallers(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredHierarchy(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredSubtypes(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredHover(query) => query.workspace_root = Some(root),
        LspSearchQuery::Document(query) => query.workspace_root = Some(root),
        LspSearchQuery::WorkspacePath(query) => query.workspace_root = Some(root),
    }
}

/// Validate the anchor file (or, with no file, the workspace root) once
/// and name it and the workspace root by their canonical paths, so a
/// continuation that spells them relative to the workspace (or absolute)
/// replays the same snapshot. Returns the canonical request path.
pub(super) fn resolve_paths(
    query: &mut LspSearchQuery,
    paths: &PathPolicy,
) -> Result<String, LspFailure> {
    if let Some(root) = lsp_search_query::workspace_root(query)
        && let Ok(valid) = paths.validate(root)
    {
        set_workspace_root(query, valid.canonical.to_string_lossy().into_owned());
    }
    let path = match lsp_search_query::path(query) {
        Some(uri) => {
            let decoded = decode_uri_path(uri).map_err(LspFailure::invalid_query)?;
            if let Some(root) = directory_as_workspace_root(query, &decoded, paths) {
                return Ok(root);
            }
            let path = paths
                .validate_read(&decoded)
                .map_err(LspFailure::path_denied)?
                .canonical
                .to_string_lossy()
                .into_owned();
            set_path(query, path.clone());
            path
        }
        None => paths
            .validate(lsp_search_query::workspace_root(query).ok_or_else(|| {
                LspFailure::invalid_query("lspSearch requires path or workspaceRoot")
            })?)
            .map_err(LspFailure::path_denied)?
            .canonical
            .to_string_lossy()
            .into_owned(),
    };
    Ok(path)
}

/// A `workspaceSymbol` `path` naming an authorized directory is read as
/// its `workspaceRoot` (a workspace-wide search has no anchor file):
/// the query keeps the directory and searches from it.
pub(super) fn directory_as_workspace_root(
    query: &mut LspSearchQuery,
    decoded: &str,
    paths: &PathPolicy,
) -> Option<String> {
    if lsp_search_query::op(query) != Op::WorkspaceSymbol {
        return None;
    }
    let directory = paths
        .validate(decoded)
        .ok()
        .filter(|valid| valid.canonical.is_dir())?
        .canonical
        .to_string_lossy()
        .into_owned();
    if lsp_search_query::workspace_root(query).is_none() {
        // A source directory (`src`) is searched from its project root,
        // where the server finds tsconfig.json, Cargo.toml, and the like.
        let root = octocode_engine::lsp::workspace::project_root_of_dir(Path::new(&directory))
            .and_then(|root| paths.validate(&root).ok())
            .filter(|valid| valid.canonical.is_dir())
            .map_or_else(
                || directory.clone(),
                |valid| valid.canonical.to_string_lossy().into_owned(),
            );
        set_workspace_root(query, root);
    }
    set_path(query, directory.clone());
    Some(directory)
}

/// Continuations repeat the caller's spelling of `workspaceRoot` (a
/// workspace-relative root stays relative), not the canonical path the
/// request resolved it to; both name the same root and snapshot.
pub(super) fn restore_root_spelling(
    row: &mut Value,
    canonical: Option<&str>,
    original: Option<&str>,
) {
    let (Some(canonical), Some(original)) = (canonical, original) else {
        return;
    };
    if canonical == original {
        return;
    }
    let Some(next) = row.get_mut("next").and_then(Value::as_object_mut) else {
        return;
    };
    for call in next.values_mut() {
        if call.get("tool").and_then(Value::as_str) != Some(crate::id::ToolId::LspSearch.as_str()) {
            continue;
        }
        let rows = call
            .pointer_mut("/query/queries")
            .and_then(Value::as_array_mut);
        for query in rows.into_iter().flatten() {
            if query.get("workspaceRoot").and_then(Value::as_str) == Some(canonical) {
                query["workspaceRoot"] = Value::from(original);
            }
        }
    }
}
