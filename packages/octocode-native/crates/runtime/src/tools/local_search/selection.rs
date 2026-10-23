//! clasifyContext for localSearch: each snippet group (one hit with its
//! context window; overlapping or touching windows in a file are one group)
//! is a candidate, judged before paging. A file stays when any of its groups
//! is selected and shows only those hits; the counts describe the kept hits,
//! and the receipt counts groups. Unknown groups stay reachable through a
//! lead to exactly the files holding them.
use super::local_search_query;
use super::cursor::stale_snapshot;
use super::local_search_query::normalized_query;
use super::local_search_query::streamed_layout;
use super::types::SearchContext;
use super::types::*;
use super::verify::{PageScope, Redacted, verify_candidate};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::tools::selection::{Candidate, Decision, Failure, Outcome, ProviderError, Runtime};
use octocode_contract_types::SelectionUnit;
use octocode_engine::types::{TextSearchFile, TextSearchResult};
use serde_json::{Value, json};
use std::collections::HashSet;

mod accounting;
#[cfg(test)]
mod tests;

use accounting::{distinct_lines, recount};

/// A selected page's plan and what it removed.
pub(super) struct Selected {
    pub(super) outcome: Outcome,
    /// Files (root-relative, scan order) holding a group judged unknown,
    /// one without a state to judge, or one the budget left unjudged.
    unknown_files: Vec<String>,
    /// Files the scan matched.
    scan_files: usize,
    /// Files that held hits of this candidate page before and after
    /// selection.
    files_before: usize,
    files_kept: usize,
    /// Hit rows considered by this candidate page, before and after
    /// selection.
    rows_before: usize,
    rows_kept: usize,
    /// The provider's error when it failed this page: the hits are shown
    /// unfiltered under an `unavailable` receipt.
    provider: Option<ProviderError>,
}

/// The query as the plan binds it and its leads carry it, with the clasify
/// context default (`local_search_query::context`) stated: an unfiltered
/// lead drops `clasifyContext` and must keep the windows the snapshot
/// captured, and a candidate page that states the default binds the same
/// plan as the call that implied it.
fn stated_query(query: &LocalSearchQuery) -> Value {
    let mut query_json = normalized_query(query, streamed_layout(query));
    if query.context_lines.is_none()
        && query.context_before.is_none()
        && query.context_after.is_none()
    {
        query_json["contextLines"] = json!(local_search_query::CLASIFY_CONTEXT_LINES);
    }
    query_json
}

/// The snippet groups of one file: indices into its `matches`, each group
/// in line order. Windows `[line - before, line + after]` that overlap or
/// touch form one group.
fn groups(file: &TextSearchFile, context: SearchContext) -> Vec<Vec<usize>> {
    let mut order = (0..file.matches.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| (file.matches[i].line, file.matches[i].column));
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut end = 0u32;
    for index in order {
        let line = file.matches[index].line;
        match groups.last_mut() {
            Some(group) if line.saturating_sub(context.before) <= end.saturating_add(1) => {
                group.push(index);
            }
            _ => groups.push(vec![index]),
        }
        end = end.max(line.saturating_add(context.after));
    }
    groups
}

/// Select snippet groups before paging; a replay requires its bound plan.
pub(super) fn select_hits(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut TextSearchResult,
    runtime: Option<&Runtime>,
    identity: &str,
    redacted: &mut Redacted,
) -> Result<Option<Selected>, ToolError> {
    let query = scope.query;
    if query.clasify_context.is_none() {
        return match query.selection {
            Some(_) => Err(stale_snapshot(query)),
            None => Ok(None),
        };
    }
    if query.snapshot.is_some() && query.selection.is_none() {
        return Err(stale_snapshot(query));
    }
    let prior = query.selection.as_ref().map(|token| token.as_str());
    let context = if super::local_search_query::uses_context(query.result_view) {
        local_search_query::context(query)
    } else {
        SearchContext::default()
    };
    let file_groups = parsed
        .files
        .iter()
        .map(|file| groups(file, context))
        .collect::<Vec<_>>();
    let total = file_groups.iter().map(Vec::len).sum();
    let needs_states = crate::tools::selection::needs_candidate_states(runtime, prior, None, total);
    let mut candidates = Vec::with_capacity(total);
    for (file, groups) in parsed.files.iter_mut().zip(&file_groups) {
        let verified = needs_states && verify_candidate(scope, file, redacted)?;
        // The declaration around each hit (the same `enclosing` a shown row
        // names), so a predicate about scope (test or production code, which
        // function) can be judged from the candidate itself.
        let outline = verified
            .then(|| {
                let source = scope.output_root.join(&file.path);
                super::enclosing::shown_outline(
                    &source,
                    (scope.expected_digest)(&source),
                    scope.security,
                )
            })
            .flatten();
        for group in groups {
            let state = verified.then(|| {
                json!({"path": file.path, "matches": group.iter().map(|&i| {
                    let hit = &file.matches[i];
                    let mut row = json!({"line":hit.line,"value":hit.value});
                    if let Some(enclosing) = outline
                        .as_ref()
                        .and_then(|outline| outline.label(outline.owner(hit.line)?))
                    {
                        row["enclosing"] =
                            json!({"kind":enclosing.kind,"symbolName":enclosing.symbol_name});
                    }
                    row
                }).collect::<Vec<_>>()})
            });
            candidates.push(Candidate::new(state));
        }
    }
    let query_json = stated_query(query);
    // The search succeeded: a provider that cannot answer now never
    // withholds its hits ([`crate::tools::selection::unless_transient`]).
    // A refused credential or request stays the row's error, with its
    // unfiltered lead.
    let (outcome, provider) =
        crate::tools::selection::unless_transient(crate::tools::selection::select(
            runtime,
            &query_json,
            identity,
            prior,
            &candidates,
            None,
            SelectionUnit::Chunks,
        ))
        .map_err(|failure| match failure {
            Failure::Stale => stale_snapshot(query),
            Failure::Cancelled => ToolError::cancelled("Selection was cancelled"),
            Failure::Provider(failure) => failure.tool_error(crate::tools::selection::unfiltered(
                ToolId::LocalSearch,
                &query_json,
                &[],
            )),
        })?;
    let unavailable = outcome.provider_unavailable();
    let mut decisions = vec![Decision::Unknown; total];
    for &ordinal in &outcome.selected {
        decisions[ordinal] = Decision::Selected;
    }
    for &ordinal in &outcome.related {
        decisions[ordinal] = Decision::Related;
    }
    for &ordinal in &outcome.excluded {
        decisions[ordinal] = Decision::Excluded;
    }
    let unjudged = outcome.unjudged.iter().copied().collect::<HashSet<_>>();
    // Unjudged groups are unknown: the unknown lead reads them as they are,
    // the next candidate page (when its token fits the `selection` field)
    // judges them.
    let (mut rows_total, mut rows_before, mut lines_removed) = (0usize, 0usize, 0usize);
    let scan_files = parsed.files.len();
    let (mut files_before, mut unknown_files) = (0usize, Vec::new());
    let mut kept = Vec::with_capacity(parsed.files.len());
    let mut ordinal = 0usize;
    for (mut file, groups) in std::mem::take(&mut parsed.files)
        .into_iter()
        .zip(file_groups)
    {
        rows_total += file.matches.len();
        let lines_before = distinct_lines(&file);
        let mut keep = vec![false; file.matches.len()];
        let (mut holds_unknown, mut judged) = (false, false);
        for group in groups {
            // A group outside this candidate page's pool was judged (and
            // shown, or left to its unfiltered route) by an earlier one.
            if outcome.considers(ordinal) {
                if unjudged.contains(&ordinal) {
                    holds_unknown = true;
                } else {
                    judged = true;
                    rows_before += group.len();
                    match decisions[ordinal] {
                        Decision::Selected => group.iter().for_each(|&i| keep[i] = true),
                        // Unjudgeable here: shown as found.
                        Decision::Unknown if unavailable => {
                            group.iter().for_each(|&i| keep[i] = true);
                        }
                        Decision::Unknown => holds_unknown = true,
                        Decision::Related | Decision::Excluded => {}
                    }
                }
            }
            ordinal += 1;
        }
        files_before += usize::from(judged);
        if holds_unknown {
            unknown_files.push(file.path.clone());
        }
        let mut flags = keep.into_iter();
        file.matches.retain(|_| flags.next().unwrap_or(false));
        lines_removed += lines_before - distinct_lines(&file);
        if !file.matches.is_empty() {
            file.match_count = file.matches.len() as u32;
            kept.push(file);
        }
    }
    parsed.files = kept;
    let rows_kept = parsed.files.iter().map(|file| file.matches.len()).sum();
    recount(
        query,
        &mut parsed.stats,
        rows_total - rows_kept,
        lines_removed,
    );
    Ok(Some(Selected {
        outcome,
        unknown_files,
        scan_files,
        files_before,
        files_kept: parsed.files.len(),
        rows_before,
        rows_kept,
        provider,
    }))
}

/// `include` entries one call accepts (the contract's `maxItems`).
fn include_cap() -> usize {
    crate::contracts::query_schema_number(ToolId::LocalSearch, None, "include", "maxItems")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0)
}

/// A glob matching exactly `path` (root-relative): a leading `/` anchors it
/// at the root and `*`/`?` are bracketed. `None` for a path a class or
/// alternation character would make ambiguous.
fn exact_glob(path: &str) -> Option<String> {
    if path.contains(['[', ']', '{', '}', '\\']) {
        return None;
    }
    let mut glob = String::with_capacity(path.len() + 1);
    glob.push('/');
    for c in path.chars() {
        if matches!(c, '*' | '?') {
            glob.push('[');
            glob.push(c);
            glob.push(']');
        } else {
            glob.push(c);
        }
    }
    Some(glob)
}
