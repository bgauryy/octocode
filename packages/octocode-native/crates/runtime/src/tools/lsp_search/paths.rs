//! Admitting the query's path and workspace root: policy checks, directory
//! roots and the caller's spelling of the root.
use super::*;

pub(super) fn set_path(query: &mut LspSearchQuery, path: String) {
    macro_rules! assign {
        ($field:expr) => {
            if let Ok(value) = path.clone().try_into() {
                *$field = value;
            }
        };
    }
    match query {
        LspSearchQuery::Variant0(q) => assign!(&mut q.path),
        LspSearchQuery::Variant1(q) => assign!(&mut q.path),
        LspSearchQuery::Variant2(q) => assign!(&mut q.path),
        LspSearchQuery::Variant3(q) => assign!(&mut q.path),
        LspSearchQuery::Variant4(q) => assign!(&mut q.path),
        LspSearchQuery::Variant5(q) => assign!(&mut q.path),
        LspSearchQuery::Variant6(q) => assign!(&mut q.path),
        LspSearchQuery::Variant7(q) => assign!(&mut q.path),
        LspSearchQuery::Variant8 { path: field, .. } => assign!(field),
        LspSearchQuery::Variant9 { path: field, .. } => assign!(field),
        LspSearchQuery::Variant10 { .. } => {}
    }
}

pub(super) fn set_workspace_root(query: &mut LspSearchQuery, root: String) {
    macro_rules! optional {
        ($field:expr) => {
            if let Ok(value) = root.clone().try_into() {
                *$field = Some(value);
            }
        };
    }
    match query {
        LspSearchQuery::Variant0(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant1(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant2(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant3(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant4(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant5(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant6(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant7(q) => optional!(&mut q.workspace_root),
        LspSearchQuery::Variant8 { workspace_root, .. } => optional!(workspace_root),
        LspSearchQuery::Variant9 { workspace_root, .. } => optional!(workspace_root),
        LspSearchQuery::Variant10 { workspace_root, .. } => {
            if let Ok(root) = root.try_into() {
                *workspace_root = root;
            }
        }
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
            // A root that does not hold the file roots the server in another
            // project: its answer would silently miss this one's importers.
            if let Some(root) = lsp_search_query::workspace_root(query)
                && Path::new(root).is_absolute()
                && !Path::new(&path).starts_with(root)
            {
                return Err(LspFailure::invalid_query(format!(
                    "workspaceRoot {root} does not contain path {path}; pass the project root above the file, or omit workspaceRoot to infer it."
                )));
            }
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
