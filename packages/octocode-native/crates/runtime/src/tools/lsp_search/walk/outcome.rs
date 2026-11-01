//! What a walk's row discloses: failed expansions, node and fan-out caps with
//! resumable continuations, and omitted results.

use super::public::public_via;
use super::{Expansion, HierarchyWalk, MAX_HIERARCHY_FAN_OUT, MAX_HIERARCHY_NODES};
use crate::tools::lsp_search::failure::{LspFailure, continuation, partial_with_related_rerun};
use crate::tools::lsp_search::{LspSearchQuery, Op, lsp_search_query};
use resume::resume_query;
use serde_json::{Value, json};

mod resume;

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
        crate::response::paging::PartialReason::TypeHierarchyExpansionFailed
    } else {
        crate::response::paging::PartialReason::CallHierarchyExpansionFailed
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
    // Most frequent first, then by message: the disclosure does not depend
    // on the order concurrent expansions failed in.
    distinct.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    // Keep every distinct provider failure beside the summary count.
    let total = failures.len();
    let items = if total == 1 { "item" } else { "items" };
    let Some((message, count)) = distinct.first() else {
        return;
    };
    let warning = if distinct.len() == 1 {
        format!("Hierarchy expansion failed for {total} {items}: {message}.")
    } else {
        format!(
            "Hierarchy expansion failed for {total} {items} ({} distinct errors; most frequent, {count}×: {message}).",
            distinct.len()
        )
    };
    if failures.iter().any(|failure| failure.retryable) {
        partial_with_related_rerun(row, query, reason, &[warning]);
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
/// way: re-anchored, it lists every result. A walk that stopped only at its
/// depth offers the next one ([`offer_deeper_walk`]).
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
        offer_deeper_walk(row, query, walks);
        return;
    }

    row["clipped"] = json!(true);
    let last_page = row.pointer("/next/nextPage").is_none();
    // Each parent resumes on its own: one that cannot be re-anchored (no
    // name or range) is a terminal limit for itself alone.
    // Many resume points share a file: each is read and line-indexed once.
    let mut sources = std::collections::HashMap::new();
    let queries = resumes
        .iter()
        .map(|resume| resume_query(query, resume, &mut sources))
        .collect::<Vec<Option<Value>>>();
    let unanchored = queries.iter().filter(|query| query.is_none()).count();
    let queries =
        (unanchored < queries.len()).then(|| queries.into_iter().flatten().collect::<Vec<_>>());
    let listed = match (last_page, queries.is_some()) {
        (false, _) => "(listed on the last page)",
        (true, true) => "(unexpandedParents, continueWalk*)",
        (true, false) => "(unexpandedParents)",
    };
    if last_page && unanchored > 0 && queries.is_some() {
        crate::response::paging::terminal_limit(
            row,
            crate::response::paging::PartialReason::HierarchyParentUnanchored,
            &[format!(
                "{unanchored} unexpanded parent(s) have no name to re-anchor on and cannot be resumed; the rest can."
            )],
        );
    }
    let limits = limit_warnings(walks, resumes.len(), listed);
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
            // The walk goes on past this page: its pages never say it ended,
            // and count only what this walk loaded.
            if let Some(pagination) = row.get_mut("pagination").and_then(Value::as_object_mut) {
                pagination.insert("hasMore".into(), Value::Bool(true));
                pagination.insert("countScope".into(), "loaded".into());
            }
        }
        None => {
            for (reason, warning) in limits {
                crate::response::paging::terminal_limit(row, reason, &[warning]);
            }
        }
    }
}

/// The lead past `depth`: the walk answered every level it was asked
/// for, but nodes at its last level were never expanded, so whether they
/// have results of their own (a subclass of a subclass, a caller's callers)
/// is unknown. `next.walkDeeper` reruns the query one level deeper from its
/// start (no cursor), on the page that ends the listing: none while a
/// remaining page (site, candidate or importer window) is offered, since
/// those list the same walk. `depth` has no maximum, so a frontier always
/// leads one level deeper.
fn offer_deeper_walk(
    row: &mut Value,
    query: &LspSearchQuery,
    walks: &[(Expansion, &HierarchyWalk)],
) {
    let depth = lsp_search_query::depth(query).unwrap_or(1);
    let frontier = walks
        .iter()
        .flat_map(|(_, walk)| &walk.edges)
        .filter(|edge| edge.level == depth)
        .count();
    let pages_remain = row
        .get("next")
        .and_then(Value::as_object)
        .is_some_and(|next| {
            next.values()
                .any(|call| call.get("kind").and_then(Value::as_str) == Some("continue"))
        });
    if frontier == 0 || pages_remain {
        return;
    }
    let mut next = lsp_search_query::to_row(query);
    let Some(object) = next.as_object_mut() else {
        return;
    };
    for field in crate::contracts::state_fields(crate::id::ToolId::LspSearch) {
        object.remove(field);
    }
    object.insert("depth".into(), json!(depth + 1));
    crate::response::result::attach_related(
        row,
        crate::response::result::Continuation::related(crate::id::ToolId::LspSearch, next).build(),
    );
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
    listed: &str,
) -> Vec<(crate::response::paging::PartialReason, String)> {
    let total = |count: fn(&HierarchyWalk) -> usize| -> usize {
        walks.iter().map(|(_, walk)| count(walk)).sum()
    };
    let dropped_edges = total(|walk| walk.dropped_edges);
    let unexpanded_nodes = total(|walk| walk.unexpanded_nodes);
    let fan_out_capped = total(|walk| walk.fan_out_capped.len());
    let candidate_capped = total(|walk| walk.candidate_capped.len());
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
            crate::response::paging::PartialReason::HierarchyNodeLimit,
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
            crate::response::paging::PartialReason::HierarchyFanOutLimit,
            format!(
                "Kept the first {MAX_HIERARCHY_FAN_OUT} results of {fan_out_capped} node(s) below the anchor {listed}; each resumes as the anchor of its continuation, which lists every result."
            ),
        ));
    }
    if candidate_capped > 0 {
        limits.push((
            crate::response::paging::PartialReason::HierarchyCandidateLimit,
            format!(
                "Checked the first {MAX_HIERARCHY_NODES} implementation candidates of {candidate_capped} type(s) below the anchor {listed}; each resumes as the anchor of its continuation, whose candidates page by window."
            ),
        ));
    }
    limits
}
