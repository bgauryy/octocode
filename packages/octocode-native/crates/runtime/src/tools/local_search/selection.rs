//! classification for localSearch: each hit is a candidate, judged before
//! paging, its context window informing the verdict. A file stays when any of
//! its hits is selected and shows only those hits; the counts and the receipt
//! describe hits. Unknown hits stay reachable through a lead to exactly the
//! files holding them.
use super::cursor::stale_snapshot;
use super::local_search_query;
use super::verify::{PageScope, Redacted, Withheld, verify_candidate};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::response::result::query_value;
use crate::tools::selection::{Candidate, Decision, Failure, Outcome, ProviderError, Runtime};
use octocode_contract_types::SelectionUnit;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{SearchContext, TextSearchFile, TextSearchMatch, TextSearchResult};
use serde_json::{Value, json};
use std::collections::HashSet;

mod accounting;
#[cfg(test)]
mod tests;

use accounting::{distinct_lines, recount};

/// A selected page's plan and what it removed.
pub(super) struct Selected {
    pub(super) outcome: Outcome,
    /// Files (root-relative, scan order) holding a hit judged unknown, one
    /// without a state to judge, or one the budget left unjudged.
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

/// The rows of `file` in source order, whatever order paging ranks them in:
/// a hit's ordinal must not move between a page and its replay.
fn source_order(file: &TextSearchFile) -> Vec<usize> {
    let mut order = (0..file.matches.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| (file.matches[i].line, file.matches[i].column));
    order
}

/// The numbered lines a row's `value` holds: its hit and context window, or
/// the whole value at the hit's line when it is not a plain window (a
/// redacted row, a grouped row, a match-only span).
fn window_lines(hit: &TextSearchMatch, context: SearchContext) -> Vec<(u32, &str)> {
    let lines = hit.value.split('\n').collect::<Vec<_>>();
    let span = hit
        .last_line
        .map_or(1, |last| last.saturating_sub(hit.line).saturating_add(1));
    let before = context.before.min(hit.line.saturating_sub(1));
    let start = hit.line.saturating_sub(before);
    let plain = hit.count.is_none()
        && hit.line > 0
        && u32::try_from(lines.len()).is_ok_and(|len| {
            len >= before.saturating_add(span) && len <= before + span + context.after
        });
    if !plain {
        return vec![(hit.line, hit.value.as_str())];
    }
    (start..).zip(lines).collect()
}

/// What the judge sees of one hit: which line it is about, and the lines that
/// inform it. Lines already in a window need no copy in the target.
fn hit_state(
    path: &str,
    hit: &TextSearchMatch,
    context: SearchContext,
    match_only: bool,
    enclosing: Option<Value>,
) -> Value {
    let window = window_lines(hit, context);
    let text = window
        .iter()
        .find(|(line, _)| *line == hit.line)
        .map_or(hit.value.as_str(), |(_, text)| *text);
    let mut target = json!({"line":hit.line, "text":text});
    if let Some(last) = hit.last_line.filter(|&last| last > hit.line) {
        target["lastLine"] = json!(last);
    }
    if match_only {
        target["match"] = json!(hit.value);
    }
    if let Some(enclosing) = enclosing {
        target["enclosing"] = enclosing;
    }
    let lines = window
        .iter()
        .map(|(line, text)| json!([line, text]))
        .collect::<Vec<_>>();
    json!({"path":path, "target":target, "lines":lines})
}

/// Select hits before paging; a replay requires its bound plan.
pub(super) fn select_hits(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut TextSearchResult,
    runtime: Option<&Runtime>,
    identity: &str,
    (redacted, withheld): (&mut Redacted, &Withheld),
) -> Result<Option<Selected>, ToolError> {
    let query = scope.query;
    if !local_search_query::has_classification(query) {
        return match local_search_query::selection(query) {
            Some(_) => Err(stale_snapshot(query)),
            None => Ok(None),
        };
    }
    if local_search_query::snapshot(query).is_some()
        && local_search_query::selection(query).is_none()
    {
        return Err(stale_snapshot(query));
    }
    let prior = local_search_query::selection(query);
    let context = if super::local_search_query::uses_context(local_search_query::view(query)) {
        local_search_query::context(query)
    } else {
        SearchContext::default()
    };
    let match_only = local_search_query::view(query) == local_search_query::View::MatchOnly;
    let file_hits = parsed.files.iter().map(source_order).collect::<Vec<_>>();
    let total = file_hits.iter().map(Vec::len).sum();
    let needs_states = crate::tools::selection::needs_candidate_states(runtime, prior, None, total);
    let mut candidates = Vec::with_capacity(total);
    for (file, hits) in parsed.files.iter_mut().zip(&file_hits) {
        let verified = needs_states && verify_candidate(scope, file, (&mut *redacted, withheld))?;
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
        for &index in hits {
            let hit = &file.matches[index];
            let state = verified.then(|| {
                let enclosing = outline
                    .as_ref()
                    .and_then(|outline| outline.label(outline.owner(hit.line)?))
                    .map(|enclosing| json!({"kind":enclosing.kind,"symbolName":enclosing.symbol_name}));
                hit_state(&file.path, hit, context, match_only, enclosing)
            });
            candidates.push(Candidate::new(state));
        }
    }
    let query_json = query_value(query);
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
            SelectionUnit::Hits,
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
    // Unjudged hits are unknown: the unknown lead reads them as they are, the
    // next candidate page judges them.
    let (mut rows_total, mut rows_before, mut lines_removed) = (0usize, 0usize, 0usize);
    let scan_files = parsed.files.len();
    let (mut files_before, mut unknown_files) = (0usize, Vec::new());
    let mut kept = Vec::with_capacity(parsed.files.len());
    let mut ordinal = 0usize;
    for (mut file, hits) in std::mem::take(&mut parsed.files).into_iter().zip(file_hits) {
        rows_total += file.matches.len();
        let lines_before = distinct_lines(&file);
        let mut keep = vec![false; file.matches.len()];
        let (mut holds_unknown, mut judged) = (false, false);
        for index in hits {
            // A hit outside this candidate page's pool was judged (and shown,
            // or left to its unfiltered route) by an earlier one.
            if outcome.considers(ordinal) {
                if unjudged.contains(&ordinal) {
                    holds_unknown = true;
                } else {
                    judged = true;
                    rows_before += 1;
                    match decisions[ordinal] {
                        Decision::Selected => keep[index] = true,
                        // Unjudgeable here: shown as found.
                        Decision::Unknown if unavailable => keep[index] = true,
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
            file.match_count = saturating_u32(file.matches.len());
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
