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
    files: &[String],
    server_root: &str,
) -> BTreeMap<PathBuf, Vec<String>> {
    let mut roots = HashMap::<&Path, PathBuf>::new();
    let root = canonical(server_root);
    let mut outside = BTreeMap::<PathBuf, Vec<String>>::new();
    for file in files {
        let dir = Path::new(file.as_str()).parent().unwrap_or(Path::new(""));
        let own = roots.entry(dir).or_insert_with(|| {
            octocode_engine::lsp::workspace::resolve_workspace_root_for_file(file.clone())
                .map_or_else(|_| dir.to_path_buf(), |own| canonical(&own))
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
    let mut others = other_projects(files, server_root);
    if let Some(dependents) = scope.known_dependents() {
        others.retain(|root, _| dependents.includes(root));
    }
    others
}

fn canonical(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.into())
}

/// A file that spells the name in code and is not in the answer may be one the
/// server never loaded: partial, with the lexical lead. With `project` (the
/// server's root), only files of another project count: one the server indexed
/// and left out holds no use. Without it (an answer listing no use, from a server
/// that may have loaded nothing), every such file counts; `page::run_target` has
/// asked the server about each and marked those whose name resolves elsewhere
/// answered, so a truly unused name stays clean however common it is.
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
    let files = scope.text_only_files(name).unwrap_or_default();
    // Ignored files are judged as the visible ones are: only a file the server
    // may not have loaded counts, so with the server's root known, one of another
    // project.
    let ignored = scope.ignored_only_files(name).map(|mut found| {
        if let Some(root) = project {
            let outside = other_projects(&found.files, root);
            found.files.retain(|file| outside.values().any(|own| own.contains(file)));
        }
        found
    });
    let ignored = ignored.filter(|found| !found.files.is_empty() || found.unfinished);
    let Some(root) = project else {
        if files.is_empty() && ignored.is_none() {
            return;
        }
        let mut note = String::new();
        if !files.is_empty() {
            note = format!(
                "The server lists no use of this name, but {} other file(s) spell it in code: its project may not cover them.",
                files.len()
            );
        }
        flag_with_ignored(row, query, scope, name, note, ignored.as_ref());
        return;
    };
    let others = dependent_projects(scope, &files, root);
    if others.is_empty() && ignored.is_none() {
        return;
    }
    flag_other_projects(row, query, scope, name, &others, ignored.as_ref());
}

/// The ignored files' share of a coverage note: counted, with the rules that
/// hide them, and a lead that reads them.
fn flag_with_ignored(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &Scope,
    name: &str,
    mut note: String,
    ignored: Option<&super::super::scope::IgnoredSpelling>,
) {
    if let Some(found) = ignored {
        if !note.is_empty() {
            note.push(' ');
        }
        note.push_str(&ignored_clause(found));
    }
    super::super::failure::flag_partial(row, query, UNANSWERED_REASON, Some(&note), scope);
    if let Some(found) = ignored
        && let Some(lead) = super::super::failure::ignored_text_lead(
            name,
            scope,
            &found.files,
            found.unfinished,
        )
    {
        crate::response::result::attach_related(row, lead);
    }
}

/// Why files under ignore rules leave the answer open: the server's project
/// does not cover them, and the text scans do not look there.
fn ignored_clause(found: &super::super::scope::IgnoredSpelling) -> String {
    let rules = match found.rules.as_slice() {
        [] => "ignore rules".to_owned(),
        rules => rules.join(", "),
    };
    let more = if found.unfinished {
        " The scan for gitignored files did not finish, so more may exist."
    } else {
        ""
    };
    format!(
        "{} gitignored file(s) ({rules}) spell this name in code, outside the project the server loaded; a localSearch with includeIgnored reads them.{more}",
        found.files.len()
    )
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
    ignored: Option<&super::super::scope::IgnoredSpelling>,
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
    let note = if others.is_empty() {
        String::new()
    } else {
        format!(
            "{} file(s) the answer leaves out spell this name in code, in projects the server did not load: {listed}. Search each in related[]; anchor lspSearch at a use there to ask that project's server.",
            others.values().map(Vec::len).sum::<usize>()
        )
    };
    flag_with_ignored(row, query, scope, name, note, ignored);
    let roots = others
        .keys()
        .map(|root| root.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for chunk in roots.chunks(super::super::failure::LISTED_TEXT_FILES) {
        if let Some(lead) =
            super::super::failure::text_search_lead(name, &scope.root, &scope.include, Some(chunk))
        {
            crate::response::result::attach_related(row, lead.build());
        }
    }
}
