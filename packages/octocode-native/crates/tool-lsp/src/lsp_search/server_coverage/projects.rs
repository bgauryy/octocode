//! Which files lie in the answering server's own project. A server indexes its
//! project, so a file there that its answer leaves out holds no use; a file of
//! another project (a sibling module, a crate the workspace excludes) is one it
//! never loaded.
use super::{LspSearchQuery, Op, Scope, UNANSWERED_REASON, lsp_search_query};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// The files of `files` (canonical) whose own server root is not `server_root`,
/// by that root. Files of one directory share a project, so each directory is
/// resolved once; a file whose root cannot be resolved stands under its own
/// directory.
pub(super) fn other_projects(
    scope: &Scope,
    files: &[String],
    server_root: &str,
) -> BTreeMap<PathBuf, Vec<String>> {
    let mut roots = HashMap::<&Path, PathBuf>::new();
    let root = super::super::scope::canonical_root(server_root);
    let mut outside = BTreeMap::<PathBuf, Vec<String>>::new();
    for file in files {
        let dir = Path::new(file.as_str()).parent().unwrap_or(Path::new(""));
        let own = roots.entry(dir).or_insert_with(|| {
            scope.project_root_of(file).map_or_else(
                |_| dir.to_path_buf(),
                |own| super::super::scope::canonical_root(&own),
            )
        });
        if *own != root {
            outside.entry(own.clone()).or_default().push(file.clone());
        }
    }
    outside
}

/// Mark answered the files of the text scan of `name` that cannot hold a use the
/// server missed: those of the project rooted at `server_root`, which it
/// indexed, and those of projects that do not depend on it
/// ([`Scope::known_dependents`]), whose spelling is a symbol of their own.
pub(in super::super) fn answer_settled_files(scope: &Scope, name: &str, server_root: &str) {
    let Some(scanned) = scope.scanned_files(name) else {
        return;
    };
    let scanned = scanned.iter().cloned().collect::<Vec<_>>();
    let open = dependent_projects(scope, &scanned, server_root)
        .into_values()
        .flatten()
        .collect::<std::collections::HashSet<_>>();
    scope.answer(scanned.into_iter().filter(|file| !open.contains(file)));
}

/// [`other_projects`] of `files` that may use the anchor ([`Scope::known_dependents`]).
pub(super) fn dependent_projects(
    scope: &Scope,
    files: &[String],
    server_root: &str,
) -> BTreeMap<PathBuf, Vec<String>> {
    let mut others = other_projects(scope, files, server_root);
    if let Some(dependents) = scope.known_dependents() {
        others.retain(|root, _| dependents.includes(root));
    }
    others
}

/// A file that spells the name in code and is not in the answer may be one the
/// server never loaded: partial, with the lexical lead. With `project` (the
/// server's root), only files of another project that may use the anchor count:
/// one the server indexed and left out holds no use, and neither does a `.rs`
/// file of its project that no module declares (it is not compiled), so both are
/// settled ([`answer_settled_files`]) and `coverage.textOnlyFiles` stops
/// counting them; every file it still counts is flagged and led to. Without it
/// (an answer listing no use, from a server that may have loaded nothing), every
/// such file counts; `unanswered::check` has asked the server about each and
/// marked those whose name resolves elsewhere answered, so a truly unused name
/// stays clean however common it is.
pub(super) fn annotate_unanswered(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &Scope,
    project: Option<&str>,
) {
    if lsp_search_query::op(query) == Op::WorkspaceSymbol {
        return;
    }
    let Some(name) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    if let Some(root) = project {
        answer_settled_files(scope, name, root);
        if row.pointer("/coverage/textOnlyFiles").is_some() {
            let left = scope.text_only_files(name).map_or(0, |files| files.len());
            super::super::failure::record_text_only(row, scope, name, left);
        }
    }
    let files = scope.text_only_files(name).unwrap_or_default();
    let Some(root) = project else {
        if files.is_empty() {
            return;
        }
        let note = format!(
            "The server lists no use of this name, but {} outside its answer spell it in code: its project may not cover them.",
            octocode_engine::text::counted(files.len(), "file", "files")
        );
        super::super::failure::flag_partial(row, query, UNANSWERED_REASON, Some(&note), scope);
        return;
    };
    let others = dependent_projects(scope, &files, root);
    if others.is_empty() {
        return;
    }
    flag_other_projects(row, query, scope, name, &others);
}

/// Partial for files of projects the server did not load: the warning names
/// every such root with its file count, and `related[]` searches each root for
/// the name, five roots per call, so every root is one call away. A use found
/// there is finished by anchoring lspSearch at it, which starts that project's
/// server.
fn flag_other_projects(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &Scope,
    name: &str,
    others: &std::collections::BTreeMap<std::path::PathBuf, Vec<String>>,
) {
    let base = Path::new(&scope.root);
    let shown = |root: &Path| {
        root.strip_prefix(base)
            .ok()
            .filter(|relative| !relative.as_os_str().is_empty())
            .map_or_else(
                || root.display().to_string(),
                |relative| relative.display().to_string(),
            )
    };
    let listed = others
        .iter()
        .map(|(root, files)| format!("{} ({})", shown(root), files.len()))
        .collect::<Vec<_>>()
        .join(", ");
    let note = format!(
        "{} file(s) the answer leaves out spell this name in code, in projects the server did not load: {listed}. The related text searches read them; anchor lspSearch at a use there to ask that project's server.",
        others.values().map(Vec::len).sum::<usize>()
    );
    super::super::failure::flag_partial(row, query, UNANSWERED_REASON, Some(&note), scope);
    let roots = others
        .keys()
        .map(|root| root.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for chunk in roots.chunks(super::super::failure::LISTED_TEXT_FILES) {
        if let Some(lead) =
            super::super::failure::text_search_lead(name, &scope.root, &scope.include, Some(chunk))
        {
            // The projects the server did not load are read only here.
            octocode_runtime_base::response::result::attach_related(row, lead.required().build());
        }
    }
}
