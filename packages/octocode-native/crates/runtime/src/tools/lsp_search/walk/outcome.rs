//! What a walk's row discloses: failed expansions, node and fan-out caps with
//! resumable continuations, and omitted results.

use super::nodes::anchor_range;
use super::public::public_via;
use super::{Expansion, HierarchyWalk, MAX_HIERARCHY_FAN_OUT, MAX_HIERARCHY_NODES, Resume};
use crate::tools::lsp_search::failure::{LspFailure, continuation, partial_with_retry};
use crate::tools::lsp_search::render::uri_to_path;
use crate::tools::lsp_search::{LspSearchQuery, Op, lsp_search_query};
use serde_json::{Value, json};

/// Combine what a hierarchy expansion found with the provider failures it hit.
/// A failure with nothing found is an error (never a silent empty answer); a
/// failure after some results is a partial answer the caller must mark.
pub(in crate::tools::lsp_search) fn expansion_outcome(
    items: Vec<Value>,
    failures: Vec<LspFailure>,
) -> Result<(Vec<Value>, Vec<LspFailure>), LspFailure> {
    if items.is_empty()
        && let Some(first) = failures.first()
    {
        return Err(first.clone());
    }
    Ok((items, failures))
}

/// Flag a walk that lost expansions partial, disclosing its failures. Only
/// a transient failure (timeout, crashed or not-ready server) offers
/// `next.retry`: an unresolved or ambiguous relation answers the same again.
pub(in crate::tools::lsp_search) fn mark_partial_expansion(
    row: &mut Value,
    query: &LspSearchQuery,
    failures: &[LspFailure],
) {
    if failures.is_empty() {
        return;
    }
    let reason = if matches!(lsp_search_query::op(query), Op::Supertypes | Op::Subtypes) {
        "typeHierarchyExpansionFailed"
    } else {
        "callHierarchyExpansionFailed"
    };
    // Every distinct failure, once, with how many items it hit.
    let mut distinct: Vec<(String, usize)> = Vec::new();
    for failure in failures {
        let message = failure.message.clone();
        match distinct.iter_mut().find(|(seen, _)| *seen == message) {
            Some((_, count)) => *count += 1,
            None => distinct.push((message, 1)),
        }
    }
    // The disclosure is one count; the error list is verbose (core field
    // class `expansionFailures`), listed whole under `debug: true`.
    let total = failures.len();
    let items = if total == 1 { "item" } else { "items" };
    let warning = match distinct.as_slice() {
        [(message, _)] => {
            format!("Hierarchy expansion failed for {total} {items}: {message}.")
        }
        _ => {
            let (message, count) =
                distinct.iter().fold(
                    &distinct[0],
                    |top, entry| if entry.1 > top.1 { entry } else { top },
                );
            format!(
                "Hierarchy expansion failed for {total} {items} ({} distinct errors; most frequent, {count}×: {message}).",
                distinct.len()
            )
        }
    };
    if failures.iter().any(|failure| failure.retryable) {
        partial_with_retry(row, query, reason, &[warning]);
    } else {
        crate::response::paging::mark_partial(row, reason, &[warning]);
    }
    if let Some(object) = row.as_object_mut() {
        object.insert(
            "expansionFailures".into(),
            json!(
                distinct
                    .iter()
                    .map(|(message, count)| format!("{count}× {message}"))
                    .collect::<Vec<_>>()
            ),
        );
    }
}

/// Truncation diagnostics of a hierarchy walk (each operation walks one
/// direction). A node-capped walk gets `clipped` and a warning on every
/// page; its last page also gets `unexpandedParents`, an executable
/// `next.continueWalk` that re-anchors on the first parent whose children
/// were dropped, and `next.continueWalk2…N` for every other such parent
/// (every page walks the same graph, so earlier pages would repeat them). A
/// node whose results were cut to [`MAX_HIERARCHY_FAN_OUT`] resumes the same
/// way: re-anchored, it lists every result.
pub(in crate::tools::lsp_search) fn mark_truncation(
    row: &mut Value,
    query: &LspSearchQuery,
    walks: &[(Expansion, &HierarchyWalk)],
) {
    if row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    note_omitted(row, walks);
    let resumes = walks
        .iter()
        .flat_map(|(_, walk)| walk.resumes.iter())
        .collect::<Vec<_>>();
    if resumes.is_empty() {
        return;
    }
    let fan_out_capped = walks
        .iter()
        .map(|(_, walk)| walk.fan_out_capped.len())
        .sum::<usize>();
    row["clipped"] = json!(true);
    let last_page = row.pointer("/next/nextPage").is_none();
    let queries = resumes
        .iter()
        .map(|resume| resume_query(query, resume))
        .collect::<Option<Vec<_>>>();
    let listed = match (last_page, queries.is_some()) {
        (false, _) => "(listed on the last page)",
        (true, true) => "(unexpandedParents, continueWalk*)",
        (true, false) => "(unexpandedParents)",
    };
    let limits = limit_warnings(walks, resumes.len(), fan_out_capped, listed);
    if last_page {
        // Every unexpanded or capped parent is listed and resumable.
        row["unexpandedParents"] = json!(
            resumes
                .iter()
                .map(|resume| {
                    let mut parent = public_via(&resume.node);
                    parent["remainingDepth"] = json!(resume.depth);
                    parent
                })
                .collect::<Vec<_>>()
        );
    }
    match queries {
        Some(queries) => {
            for (reason, warning) in limits {
                crate::response::paging::mark_partial(row, reason, &[warning]);
            }
            if !last_page {
                return;
            }
            // One flat continuation per parent, so the whole frontier is
            // executable: `continueWalk`, `continueWalk2`, ….
            for (index, query) in queries.into_iter().enumerate() {
                let key = match index {
                    0 => "continueWalk".to_owned(),
                    _ => format!("continueWalk{}", index + 1),
                };
                row["next"][key] = continuation(query);
            }
        }
        None => {
            for (reason, warning) in limits {
                crate::response::paging::terminal_limit(row, reason, &[warning]);
            }
        }
    }
}

/// Every walk's TypeScript built-in library callees, each named once.
pub(super) fn builtin_lib_names<'a>(walks: &[(Expansion, &'a HierarchyWalk)]) -> Vec<&'a String> {
    let mut names: Vec<&String> = Vec::new();
    for name in walks.iter().flat_map(|(_, walk)| &walk.builtin_lib) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// What the walk left out of its items: a warning for results outside the
/// read policy; TypeScript built-in library callees are listed by name
/// under `builtinLib` (tiny, and never worth a second call).
fn note_omitted(row: &mut Value, walks: &[(Expansion, &HierarchyWalk)]) {
    let out_of_policy = walks
        .iter()
        .map(|(_, walk)| walk.out_of_policy)
        .sum::<usize>();
    if out_of_policy > 0
        && let Some(warnings) = row
            .as_object_mut()
            .map(|object| object.entry("warnings").or_insert_with(|| json!([])))
            .and_then(Value::as_array_mut)
    {
        warnings.push(json!(format!(
            "{out_of_policy} hierarchy items outside the allowed read roots were omitted."
        )));
    }
    let builtin_lib = builtin_lib_names(walks);
    if !builtin_lib.is_empty()
        && let Some(payload) = row.as_object_mut()
    {
        payload.insert("builtinLib".into(), json!(builtin_lib));
    }
}

/// `(partial reason, warning)` for each cap a walk hit; `parents` resume
/// points are `listed` where the warning says.
fn limit_warnings(
    walks: &[(Expansion, &HierarchyWalk)],
    parents: usize,
    fan_out_capped: usize,
    listed: &str,
) -> Vec<(&'static str, String)> {
    let dropped_edges = walks
        .iter()
        .map(|(_, walk)| walk.dropped_edges)
        .sum::<usize>();
    let unexpanded_nodes = walks
        .iter()
        .map(|(_, walk)| walk.unexpanded_nodes)
        .sum::<usize>();
    let mut limits = Vec::new();
    let mut cut = Vec::new();
    if dropped_edges > 0 {
        cut.push(format!(
            "{dropped_edges} edges to further nodes were not listed"
        ));
    }
    if unexpanded_nodes > 0 {
        cut.push(format!(
            "{unexpanded_nodes} listed results were not expanded"
        ));
    }
    if !cut.is_empty() {
        limits.push((
            "hierarchyNodeLimit",
            format!(
                "Stopped at {MAX_HIERARCHY_NODES} hierarchy nodes: {}; {parents} parents were left unexpanded {listed}.",
                cut.join(" and ")
            ),
        ));
    }
    // The parent list names every capped node once; the warning carries
    // the count.
    if fan_out_capped > 0 {
        limits.push((
            "hierarchyFanOutLimit",
            format!(
                "Kept the first {MAX_HIERARCHY_FAN_OUT} results of {fan_out_capped} node(s) below the anchor {listed}; each resumes as the anchor of its continuation, which lists every result."
            ),
        ));
    }
    limits
}

/// An executable query that walks `resume.depth` levels from `resume.node`:
/// the same operation re-anchored on the node by its name and 1-based selection line, with
/// `orderHint` when the name repeats on that line before the node (no
/// 0-based coordinate reaches an input).
fn resume_query(query: &LspSearchQuery, resume: &Resume) -> Option<Value> {
    let uri = resume.node.get("uri").and_then(Value::as_str)?;
    let name = resume.node.get("name").and_then(Value::as_str)?;
    let anchor = anchor_range(&resume.node)?;
    let line = u32::try_from(anchor.pointer("/start/line")?.as_u64()?).ok()?;
    let character = u32::try_from(anchor.pointer("/start/character")?.as_u64()?).ok()?;
    let path = uri_to_path(uri);
    let mut next = lsp_search_query::to_row(query);
    let object = next.as_object_mut()?;
    object.insert("path".into(), json!(path));
    for field in ["orderHint", "page", "snapshot", "importerPage"] {
        object.remove(field);
    }
    object.insert("symbolName".into(), json!(name));
    object.insert("line".into(), json!(line.saturating_add(1)));
    if let Some(order) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| order_on_line(&text, line, name, character))
        .filter(|order| *order > 0)
    {
        object.insert("orderHint".into(), json!(order));
    }
    object.insert("graphDepth".into(), json!(resume.depth));
    Some(next)
}

/// How many whole-word occurrences of `name` start before the UTF-16
/// `character` on 0-based `line` of `text`: the `orderHint` that re-anchors
/// the node. `None` when the line is absent.
fn order_on_line(text: &str, line: u32, name: &str, character: u32) -> Option<u32> {
    let row = text.lines().nth(usize::try_from(line).ok()?)?;
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut order = 0u32;
    let mut units = 0u32;
    for (index, c) in row.char_indices() {
        if units >= character {
            break;
        }
        if row[index..].starts_with(name)
            && !row[..index].chars().next_back().is_some_and(is_word)
            && !row[index + name.len()..]
                .chars()
                .next()
                .is_some_and(is_word)
        {
            order += 1;
        }
        units += u32::try_from(c.len_utf16()).ok()?;
    }
    Some(order)
}
