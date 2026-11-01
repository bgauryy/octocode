//! Re-anchoring a capped node: the query that resumes the walk from it.

use super::super::Resume;
use super::super::nodes::anchor_range;
use crate::tools::lsp_search::render::uri_to_path;
use crate::tools::lsp_search::source::Source;
use crate::tools::lsp_search::{LspSearchQuery, lsp_search_query};
use serde_json::{Value, json};
use std::collections::HashMap;

/// An executable query that walks `resume.depth` levels from `resume.node`:
/// the same operation re-anchored on the node by its name and 1-based selection line, with
/// `occurrence` when the name repeats on that line before the node (no
/// 0-based coordinate reaches an input). `sources` keeps each file read for
/// the request's other resume points, split into lines as the server
/// counts them.
pub(super) fn resume_query(
    query: &LspSearchQuery,
    resume: &Resume,
    sources: &mut HashMap<String, Option<Source>>,
) -> Option<Value> {
    let uri = resume.node.get("uri").and_then(Value::as_str)?;
    let name = resume.node.get("name").and_then(Value::as_str)?;
    let anchor = anchor_range(&resume.node)?;
    let line = u32::try_from(anchor.pointer("/start/line")?.as_u64()?).ok()?;
    let character = u32::try_from(anchor.pointer("/start/character")?.as_u64()?).ok()?;
    let path = uri_to_path(uri);
    let mut next = lsp_search_query::to_row(query);
    let object = next.as_object_mut()?;
    object.insert("path".into(), json!(path));
    // Re-anchored, the node starts its own walk: no page, window or
    // candidate cursor of this one carries over.
    for field in [
        "occurrence",
        "page",
        "snapshot",
        "importerPage",
        "candidatePage",
    ] {
        object.remove(field);
    }
    object.insert("symbolName".into(), json!(name));
    object.insert("line".into(), json!(line.saturating_add(1)));
    let source = sources
        .entry(path.clone())
        .or_insert_with(|| std::fs::read_to_string(&path).ok().map(Source::new));
    if let Some(order) = source
        .as_ref()
        .and_then(|source| order_on_line(source, line, name, character))
        .filter(|order| *order > 0)
    {
        object.insert("occurrence".into(), json!(order));
    }
    object.insert("depth".into(), json!(resume.depth));
    Some(next)
}

/// How many whole-word occurrences of `name` start before the UTF-16
/// `character` on 0-based `line` of `source`: the `occurrence` that
/// re-anchors the node. `None` when the line is absent.
fn order_on_line(source: &Source, line: u32, name: &str, character: u32) -> Option<u32> {
    let line = usize::try_from(line).ok()?;
    if line >= source.line_count() {
        return None;
    }
    let row = source.lines(line, line + 1);
    let mut order = 0u32;
    let mut units = 0u32;
    for (index, c) in row.char_indices() {
        if units >= character {
            break;
        }
        if row[index..].starts_with(name)
            && octocode_engine::text::is_whole_word(row.as_bytes(), index, index + name.len())
        {
            order += 1;
        }
        units += u32::try_from(c.len_utf16()).ok()?;
    }
    Some(order)
}

#[cfg(test)]
mod tests;
