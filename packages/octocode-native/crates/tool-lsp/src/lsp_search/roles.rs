//! The role of each reference site (`role`): the symbol's declaration, a
//! call of it, an import or re-export binding, or another use. A site that
//! starts a declaration of the file's declarations parse is a declaration
//! ([`binding_role`]), and one on a name its imports bind is an import
//! ([`binding_role`], each line of a multi-line import group). JS/TS sites read one oxc parse of their file
//! ([`classify_sites`]); every other site, and a name the parse holds no
//! node for, reads the text rule of its own line. In a file with no
//! declarations parse (a language octocode has no grammar for), the
//! server's own outline names its declarations, as the callers walk reads it.
use super::call_sites::text_rule::is_call_site;
use super::call_sites::{classify_sites, flatten_symbols, is_declaration_name};
use super::failure::LspFailure;
use super::recovery::{BindingRole, binding_role};
use super::render::uri_to_path;
use super::source::SourceCache;
use octocode_engine::graph::GraphPosition;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::types::LspRange;
use octocode_engine::text::LineIndex;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Set `role` on each location row of `rows` (LSP `uri` and 0-based
/// `range`). A row keeps no role only when its file's source cannot be
/// read here (outside the read policy, unreadable, or over the size bound):
/// no rule can classify a site whose text is not available. `server`, when
/// one answers, gives the outline of a file without a declarations parse.
pub(super) async fn assign_roles(
    rows: &mut [Value],
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    server: Option<&NativeLspClient>,
) -> Result<(), LspFailure> {
    let mut by_file = BTreeMap::<String, Vec<usize>>::new();
    for (index, row) in rows.iter().enumerate() {
        if let Some(uri) = row["uri"].as_str() {
            by_file.entry(uri_to_path(uri)).or_default().push(index);
        }
    }
    for (path, indices) in by_file {
        let Some(source) = sources.get(&path).await else {
            continue;
        };
        let point = |row: &Value, edge: &str| GraphPosition {
            line: octocode_engine::num::saturating_u32(
                row["range"][edge]["line"].as_u64().unwrap_or(0),
            ),
            character: octocode_engine::num::saturating_u32(
                row["range"][edge]["character"].as_u64().unwrap_or(0),
            ),
        };
        let starts = indices
            .iter()
            .map(|index| point(&rows[*index], "start"))
            .collect::<Vec<_>>();
        let declared = parsed_roles(cancel, &path, &source, &starts).await?;
        let parsed = classify_sites(cancel, &source, &path, starts).await?;
        let lines = LineIndex::new(&source.content);
        let mut text_decided = Vec::new();
        let parsed_roles = declared.as_deref().unwrap_or_default();
        for (position, &index) in indices.iter().enumerate() {
            let site = parsed.as_ref().and_then(|sites| sites.get(position));
            let row = &mut rows[index];
            let (start, end) = (point(row, "start"), point(row, "end"));
            let text = || {
                if imports_on(line_text(&source.content, &lines, start.line)) {
                    "import"
                } else if is_call_site(
                    &source.content,
                    &lines,
                    (start.line, start.character),
                    (end.line, end.character),
                ) {
                    "call"
                } else {
                    "reference"
                }
            };
            let mut by_text = || {
                text_decided.push(index);
                text()
            };
            let role = match (parsed_roles.get(position).copied().flatten(), site) {
                (Some(BindingRole::Declaration), _) => "declaration",
                (Some(BindingRole::ImportedName), _) => {
                    row["importRole"] = json!("importedName");
                    "import"
                }
                (Some(BindingRole::LocalAlias), _) => {
                    row["importRole"] = json!("localAlias");
                    "import"
                }
                (Some(BindingRole::Import), _) => "import",
                (None, Some(site)) if site.is_import == Some(true) => "import",
                (None, Some(site)) => match site.is_call {
                    Some(true) => "call",
                    Some(false) if site.is_import == Some(false) => "reference",
                    _ => by_text(),
                },
                (None, None) => by_text(),
            };
            row["role"] = json!(role);
        }
        // No declarations parse: the text rule cannot tell a declaration's
        // name (`func f(`) from a call; the server's outline can. A failed
        // outline leaves the text rule's roles.
        if declared.is_none()
            && !text_decided.is_empty()
            && let Some(server) = server
            && let Ok(outline) = server.get_document_symbols(path.clone()).await
        {
            mark_declarations(rows, &text_decided, &outline);
        }
    }
    Ok(())
}

/// Whether each zero-based `(line, character)` site of `path` is an import
/// binding (`use a::{f}`, `import { f }`), by the rules [`assign_roles`]
/// reads: the declarations parse, else the text of the site's line. `None`
/// when the file's source cannot be read here: no rule can classify it.
pub(super) async fn import_sites(
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    path: &str,
    sites: &[(u32, u32)],
) -> Result<Option<Vec<bool>>, LspFailure> {
    let Some(source) = sources.get(path).await else {
        return Ok(None);
    };
    let starts = sites
        .iter()
        .map(|&(line, character)| GraphPosition { line, character })
        .collect::<Vec<_>>();
    let parsed = parsed_roles(cancel, path, &source, &starts)
        .await?
        .unwrap_or_default();
    let lines = LineIndex::new(&source.content);
    Ok(Some(
        sites
            .iter()
            .enumerate()
            .map(
                |(index, &(line, _))| match parsed.get(index).copied().flatten() {
                    Some(role) => role != BindingRole::Declaration,
                    None => imports_on(line_text(&source.content, &lines, line)),
                },
            )
            .collect(),
    ))
}

/// Set `role: declaration` on each row of `indices` whose range is a
/// declaration's own name in the server's `outline`.
fn mark_declarations(rows: &mut [Value], indices: &[usize], outline: &Value) {
    let mut flat = Vec::new();
    flatten_symbols(outline, &mut flat);
    for &index in indices {
        let named = serde_json::from_value::<LspRange>(rows[index]["range"].clone())
            .is_ok_and(|range| is_declaration_name(&flat, &range));
        if named {
            rows[index]["role"] = json!("declaration");
        }
    }
}

/// The role the file's digest-cached declarations parse gives each of
/// `starts`, read off the async workers: `declaration` where it begins a
/// declaration (its name or its start), `import` on a name an import
/// binds, else `None`; `None` for a file without one.
async fn parsed_roles(
    cancel: &dyn CancellationCheck,
    path: &str,
    source: &std::sync::Arc<super::source::Source>,
    starts: &[GraphPosition],
) -> Result<Option<Vec<Option<BindingRole>>>, LspFailure> {
    let (path, source) = (path.to_owned(), std::sync::Arc::clone(source));
    let starts = starts
        .iter()
        .map(|start| (start.line, start.character))
        .collect::<Vec<_>>();
    let roles = super::blocking_cancellable(cancel, move |_| {
        let document = octocode_tools_shared::declarations::file_document(&source.content, &path)?;
        Some(
            starts
                .iter()
                .map(|&(line, character)| binding_role(&document, line, character))
                .collect::<Vec<_>>(),
        )
    })
    .await?;
    Ok(roles.flatten())
}

/// The text of 0-based `line` of `content`, without its line break.
fn line_text<'a>(content: &'a str, lines: &LineIndex, line: u32) -> &'a str {
    let at = |line: u32| {
        usize::try_from(line)
            .ok()
            .and_then(|line| lines.line_start(line))
    };
    let start = at(line).unwrap_or(content.len());
    let end = line.checked_add(1).and_then(at).unwrap_or(content.len());
    content.get(start..end).unwrap_or_default()
}

/// The text rule of an import: the site's line opens a statement that
/// brings names from another module (`use`, `import`, `from … import`,
/// `export … from`, `#include`). A line inside a multi-line import group
/// opens no statement: the declarations parse places it ([`binding_role`]).
fn imports_on(line: &str) -> bool {
    let line = line.trim_start();
    let line = ["pub(crate) ", "pub(super) ", "pub(self) ", "pub "]
        .iter()
        .find_map(|visibility| line.strip_prefix(visibility))
        .unwrap_or(line);
    line.starts_with("use ")
        || line.starts_with("import ")
        || line.starts_with("#include")
        || (line.starts_with("from ") && line.contains(" import "))
        || (line.starts_with("export ") && line.contains(" from "))
}

#[cfg(test)]
mod tests;
