//! What a walk's row discloses: failed expansions, node and fan-out caps with
//! resumable continuations, and omitted results.

use super::public::{public_edge, public_via};
use super::{Expansion, HierarchyWalk, MAX_HIERARCHY_FAN_OUT, MAX_HIERARCHY_NODES};
use crate::lsp_search::failure::{LspFailure, partial_with_related_rerun};
use crate::lsp_search::test_scope;
use crate::lsp_search::{LspSearchQuery, Op, lsp_search_query};
use octocode_contract_types::LspSearchChannel;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::paths::shown_path;
use octocode_runtime_base::response::result::{Continuation, insert_next};
use resume::resume_query;
use serde_json::{Value, json};

mod resume;

/// Combine what a hierarchy expansion found with the provider failures it hit.
/// A failure with nothing found is an error (never a silent empty answer); a
/// failure after some results is a partial answer the caller must mark.
pub(in crate::lsp_search) fn expansion_outcome(
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
/// The failures are stated once: `expansionFailures` lists each distinct
/// error with its count, spelling paths as the rows do ([`shown_text`]),
/// beside `partialReasons`; no warning restates them.
pub(in crate::lsp_search) fn mark_partial_expansion(
    row: &mut Value,
    query: &LspSearchQuery,
    failures: &[LspFailure],
    paths: &PathPolicy,
) {
    if failures.is_empty() {
        return;
    }
    let reason = if matches!(lsp_search_query::op(query), Op::Supertypes | Op::Subtypes) {
        octocode_runtime_base::response::paging::PartialReason::TypeHierarchyExpansionFailed
    } else {
        octocode_runtime_base::response::paging::PartialReason::CallHierarchyExpansionFailed
    };
    // Every distinct failure, once, with how many items it hit.
    let mut distinct: Vec<(String, usize)> = Vec::new();
    for failure in failures {
        let message = shown_text(paths, &failure.message);
        match distinct.iter_mut().find(|(seen, _)| *seen == message) {
            Some((_, count)) => *count += 1,
            None => distinct.push((message, 1)),
        }
    }
    // Most frequent first, then by message: the disclosure does not depend
    // on the order concurrent expansions failed in.
    distinct.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if failures.iter().any(|failure| failure.retryable) {
        partial_with_related_rerun(row, query, reason, &[]);
    } else {
        octocode_runtime_base::response::paging::mark_partial(row, reason, &[]);
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

/// `message` with each absolute path in it named as the rows name it
/// ([`shown_path`]): from the workspace inside it, from the root of its tree
/// outside it, as given when no root holds it. A path runs from a `/` that
/// starts a word (after a space, a quote or a bracket) to the end of the
/// word, less a `:line` or `:line:column` and closing punctuation.
fn shown_text(paths: &PathPolicy, message: &str) -> String {
    paths
        .workspace_relative_text(message)
        .split_inclusive(char::is_whitespace)
        .map(|piece| shown_word(paths, piece))
        .collect()
}

/// One whitespace-ended piece of a message, its path named by [`shown_path`].
fn shown_word(paths: &PathPolicy, piece: &str) -> String {
    let word = piece.trim_end();
    let space = piece.strip_prefix(word).unwrap_or_default();
    let opened = word.trim_start_matches(['\'', '"', '(', '`']);
    let open = word.strip_suffix(opened).unwrap_or_default();
    let inner = opened.trim_end_matches(['\'', '"', ')', '`', ',', ';', '.']);
    let close = opened.strip_prefix(inner).unwrap_or_default();
    let mut path = inner;
    for _ in 0..2 {
        if let Some((head, number)) = path.rsplit_once(':')
            && !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit())
        {
            path = head;
        }
    }
    if !std::path::Path::new(path).is_absolute() {
        return piece.to_owned();
    }
    let place = inner.strip_prefix(path).unwrap_or_default();
    format!("{open}{}{place}{close}{space}", shown_path(paths, path))
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
pub(in crate::lsp_search) fn mark_truncation(
    row: &mut Value,
    query: &LspSearchQuery,
    walks: &[(Expansion, &HierarchyWalk)],
    paths: &PathPolicy,
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
        offer_deeper_walk(row, query, walks, paths);
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
        .collect::<Vec<Option<LspSearchQuery>>>();
    let unanchored = queries.iter().filter(|query| query.is_none()).count();
    let queries =
        (unanchored < queries.len()).then(|| queries.into_iter().flatten().collect::<Vec<_>>());
    let listed = match (last_page, queries.is_some()) {
        (false, _) => "(listed on the last page)",
        (true, true) => "(unexpandedParents, continueWalk*)",
        (true, false) => "(unexpandedParents)",
    };
    if last_page && unanchored > 0 && queries.is_some() {
        octocode_runtime_base::response::paging::terminal_limit(
            row,
            octocode_runtime_base::response::paging::PartialReason::HierarchyParentUnanchored,
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
                octocode_runtime_base::response::paging::mark_partial(row, reason, &[warning]);
            }
            if !last_page {
                return;
            }
            // One flat continuation per parent, so the whole frontier is
            // executable: `continueWalk`, `continueWalk2`, ….
            for (index, query) in queries.into_iter().enumerate() {
                let call = Continuation::continued_query(query);
                match index {
                    0 => insert_next(&mut row["next"], LspSearchChannel::ContinueWalk, call),
                    _ => octocode_runtime_base::response::result::set_next(
                        row,
                        &format!("continueWalk{}", index + 1),
                        call.build(),
                    ),
                }
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
                octocode_runtime_base::response::paging::terminal_limit(row, reason, &[warning]);
            }
        }
    }
}

/// The lead past `depth`: the walk answered every level it was asked
/// for, but nodes first reached at its last level were never expanded, so
/// whether they have results of their own (a subclass of a subclass, a
/// caller's callers) is unknown. A last level of nodes the walk already
/// expanded (a cycle) leaves nothing unknown and offers no lead. `next.walkDeeper` reruns the query one level deeper from its
/// start (no cursor), on the page that ends the listing: none while a
/// remaining page (site, candidate or importer window) is offered, since
/// those list the same walk. `depth` has no maximum, so a frontier always
/// leads one level deeper. The frontier is the row's: the nodes its
/// `testCode` keeps ([`test_scope::retain_callers`], the filter the items
/// passed). A last level the scope left out whole leads instead to the call
/// that lists it, `testCode:"include"`: a deeper walk answers the same row.
fn offer_deeper_walk(
    row: &mut Value,
    query: &LspSearchQuery,
    walks: &[(Expansion, &HierarchyWalk)],
    paths: &PathPolicy,
) {
    let depth = lsp_search_query::depth(query).unwrap_or(1);
    let last_level = |unexpanded_only: bool| {
        walks
            .iter()
            .flat_map(|(expansion, walk)| {
                walk.edges
                    .iter()
                    .filter(|edge| edge.level == depth)
                    .filter(|edge| {
                        !unexpanded_only || walk.unexpanded_last_level.contains(&edge.node)
                    })
                    .map(|edge| public_edge(*expansion, edge, None))
            })
            .collect::<Vec<_>>()
    };
    let reached = last_level(false).len();
    // Only a node the walk never expanded can have results of its own.
    let mut frontier = last_level(true);
    let unexpanded = frontier.len();
    test_scope::retain_callers(query, paths, &mut frontier);
    let pages_remain = row
        .get("next")
        .and_then(Value::as_object)
        .is_some_and(|next| {
            next.values()
                .any(|call| call.get("kind").and_then(Value::as_str) == Some("continue"))
        });
    if reached == 0 || pages_remain {
        return;
    }
    // A scope that left out every last-level edge leads to the call that
    // lists them; a last level of already-expanded nodes leads nowhere.
    let change = if frontier.is_empty() && unexpanded > 0 {
        json!({"testCode": "include"})
    } else if frontier.is_empty() {
        return;
    } else {
        json!({"depth": depth + 1})
    };
    // A related request is fresh: it drops this walk's cursors itself.
    let Ok(lead) = lsp_search_query::edited(query, &change) else {
        return;
    };
    octocode_runtime_base::response::result::attach_related(
        row,
        Continuation::related_query(lead).build(),
    );
}

/// Every walk's bases declared outside the read policy, each named once.
pub(super) fn withheld_names<'a>(walks: &[(Expansion, &'a HierarchyWalk)]) -> Vec<&'a String> {
    let mut names: Vec<&String> = Vec::new();
    for name in walks.iter().flat_map(|(_, walk)| &walk.withheld_names) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
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
    if out_of_policy > 0 {
        // Missing callers: the same count `references` reports as partial.
        // A base the source names is named: its name is read from source
        // the policy allows.
        let named = withheld_names(walks);
        let named = if named.is_empty() {
            String::new()
        } else {
            let names = named.iter().map(|name| name.as_str()).collect::<Vec<_>>();
            format!(" ({})", names.join(", "))
        };
        octocode_runtime_base::response::paging::mark_partial(
            row,
            octocode_runtime_base::response::paging::PartialReason::LocationsWithheld,
            &[format!(
                "{out_of_policy} hierarchy item(s) outside the read policy omitted{named}."
            )],
        );
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
) -> Vec<(
    octocode_runtime_base::response::paging::PartialReason,
    String,
)> {
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
            "{} to further nodes not listed",
            octocode_engine::text::counted(dropped_edges, "edge", "edges")
        ));
    }
    if unexpanded_nodes > 0 {
        cut.push(format!(
            "{} not expanded",
            octocode_engine::text::counted(unexpanded_nodes, "listed result", "listed results")
        ));
    }
    if !cut.is_empty() {
        limits.push((
            octocode_runtime_base::response::paging::PartialReason::HierarchyNodeLimit,
            format!(
                "Stopped at {MAX_HIERARCHY_NODES} hierarchy nodes: {}; {} left unexpanded {listed}.",
                cut.join(" and "),
                octocode_engine::text::counted(parents, "parent", "parents")
            ),
        ));
    }
    // The parent list names every capped node once; the warning carries
    // the count.
    if fan_out_capped > 0 {
        limits.push((
            octocode_runtime_base::response::paging::PartialReason::HierarchyFanOutLimit,
            format!(
                "Kept the first {MAX_HIERARCHY_FAN_OUT} results of {fan_out_capped} node(s) below the anchor {listed}; each resumes as the anchor of its continuation, which lists every result."
            ),
        ));
    }
    if candidate_capped > 0 {
        limits.push((
            octocode_runtime_base::response::paging::PartialReason::HierarchyCandidateLimit,
            format!(
                "Checked the first {MAX_HIERARCHY_NODES} implementation candidates of {candidate_capped} type(s) below the anchor {listed}; each resumes as the anchor of its continuation, whose candidates page by window."
            ),
        ));
    }
    limits
}
