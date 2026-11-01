//! The role of each reference site (`role`): the symbol's declaration, a
//! call of it, an import or re-export binding, or another use. A site that
//! starts a declaration of the file's declarations parse is a declaration
//! ([`lands`]). JS/TS sites read one oxc parse of their file
//! ([`classify_sites`]); every other site, and a name the parse holds no
//! node for, reads the text rule of its own line.
use super::call_sites::{classify_sites, is_call_at};
use super::failure::LspFailure;
use super::recovery::{Landing, lands};
use super::render::uri_to_path;
use super::source::SourceCache;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::graph::GraphPosition;
use octocode_engine::text::LineIndex;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Set `role` on each location row of `rows` (LSP `uri` and 0-based
/// `range`). A row keeps no role only when its file's source cannot be
/// read here (outside the read policy, unreadable, or over the size bound):
/// no rule can classify a site whose text is not available.
pub(super) async fn assign_roles(
    rows: &mut [Value],
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
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
            line: crate::num::u32_of(row["range"][edge]["line"].as_u64().unwrap_or(0)),
            character: crate::num::u32_of(row["range"][edge]["character"].as_u64().unwrap_or(0)),
        };
        let starts = indices
            .iter()
            .map(|index| point(&rows[*index], "start"))
            .collect::<Vec<_>>();
        let declared = declared_sites(cancel, &path, &source, &starts).await?;
        let parsed = classify_sites(cancel, &source, &path, starts).await?;
        let lines = LineIndex::new(&source.content);
        for (position, index) in indices.into_iter().enumerate() {
            let site = parsed.as_ref().and_then(|sites| sites.get(position));
            let row = &mut rows[index];
            let (start, end) = (point(row, "start"), point(row, "end"));
            let text = || {
                if imports_on(line_text(&source.content, &lines, start.line)) {
                    "import"
                } else if is_call_at(&source.content, &lines, end.line, end.character) {
                    "call"
                } else {
                    "reference"
                }
            };
            let role = match site {
                _ if declared.get(position) == Some(&true) => "declaration",
                Some(site) if site.is_import == Some(true) => "import",
                Some(site) => match site.is_call {
                    Some(true) => "call",
                    Some(false) if site.is_import == Some(false) => "reference",
                    _ => text(),
                },
                None => text(),
            };
            row["role"] = json!(role);
        }
    }
    Ok(())
}

/// Whether each of `starts` begins a declaration (its name or its start) of
/// the file's digest-cached declarations parse, read off the async workers;
/// all `false` for a file without one.
async fn declared_sites(
    cancel: &dyn CancellationCheck,
    path: &str,
    source: &std::sync::Arc<super::source::Source>,
    starts: &[GraphPosition],
) -> Result<Vec<bool>, LspFailure> {
    let (path, source) = (path.to_owned(), std::sync::Arc::clone(source));
    let starts = starts
        .iter()
        .map(|start| (start.line, start.character))
        .collect::<Vec<_>>();
    let declared = super::blocking_cancellable(cancel, move |_| {
        let document = crate::tools::declarations_cache::file_document(&source.content, &path);
        starts
            .iter()
            .map(|&(line, character)| {
                document.as_ref().is_some_and(|document| {
                    lands(document, line, character) == Landing::Declaration
                })
            })
            .collect::<Vec<_>>()
    })
    .await?;
    Ok(declared.unwrap_or_default())
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
/// opens no statement, so it reads as a reference.
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
