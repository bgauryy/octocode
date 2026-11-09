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
use std::collections::{HashMap, HashSet};

mod accounting;
#[cfg(test)]
mod tests;

use accounting::{distinct_lines, recount};

/// A selected page's plan and what it removed.
pub(super) struct Selected {
    pub(super) outcome: Outcome,
    /// Hit rows considered by this candidate page, before and after
    /// selection.
    rows_before: usize,
    rows_kept: usize,
    /// Files with a hit before selection, and of them the files this page
    /// judged every hit of and kept none: `next.unfiltered` alone lists
    /// those (a file with a hit left to judge waits for its candidate page).
    files_before: usize,
    files_dropped: usize,
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

/// The numbered lines a row's `value` holds: its hit and context window,
/// from the line the search says it starts at (`first_line`), or the whole
/// value at the hit's line when it is not a plain window (a redacted row, a
/// grouped row, a match-only span, lines that disagree with that window).
fn window_lines(hit: &TextSearchMatch, context: SearchContext) -> Vec<(u32, &str)> {
    let lines = hit.value.split('\n').collect::<Vec<_>>();
    let span = hit
        .last_line
        .map_or(1, |last| last.saturating_sub(hit.line).saturating_add(1));
    let plain = hit.count.is_none()
        && hit.first_line > 0
        && hit.line.checked_sub(hit.first_line).is_some_and(|before| {
            before <= context.before
                && u32::try_from(lines.len()).is_ok_and(|len| {
                    len >= before.saturating_add(span)
                        && len <= before.saturating_add(span).saturating_add(context.after)
                })
        });
    if !plain {
        return vec![(hit.line, hit.value.as_str())];
    }
    (hit.first_line..).zip(lines).collect()
}

/// What the judge reads of a verified file beside each hit's row: the
/// declaration around a hit, the key path of its line in a keyed file (JSON,
/// TOML), and, for a matchOnly row whose value is only the matched span, the
/// lines as a read shows them.
#[derive(Default)]
struct FileEvidence {
    outline: Option<super::enclosing::Outline>,
    keys: HashMap<u32, String>,
    lines: Option<Vec<String>>,
}

impl FileEvidence {
    /// One read of the file at `path`, the bytes the scan matched.
    fn read(
        scope: &PageScope<'_, impl CancellationCheck>,
        path: &str,
        hit_lines: &[usize],
        match_only: bool,
    ) -> Self {
        let source = scope.output_root.join(path);
        let Some(bytes) = super::enclosing::shown_bytes(&source, (scope.expected_digest)(&source))
        else {
            return Self::default();
        };
        let outline = super::enclosing::outline_of(&bytes, &source, scope.security);
        let limit = crate::security::SourceLimits::REAL.whole_source;
        let Ok(text) = scope.security.decode_source_bytes(&bytes, limit) else {
            return Self {
                outline,
                ..Self::default()
            };
        };
        let keys = crate::tools::match_keys::line_key_paths(&source, &text, hit_lines)
            .into_iter()
            .filter_map(|(line, key)| Some((u32::try_from(line).ok()?, key)))
            .collect();
        let lines = match_only
            .then(|| shown_lines(&text, &source, scope.security))
            .flatten();
        Self {
            outline,
            keys,
            lines,
        }
    }

    /// The source lines `first..=last` as a read shows them.
    fn span(&self, first: u32, last: u32) -> Option<Vec<(u32, &str)>> {
        let lines = self.lines.as_ref()?;
        (first..=last)
            .map(|line| {
                let at = usize::try_from(line.checked_sub(1)?).ok()?;
                Some((line, lines.get(at)?.as_str()))
            })
            .collect()
    }
}

/// `text`'s lines as a read shows them, private keys and secrets redacted;
/// `None` when redaction changed the line count, so no line keeps its number.
fn shown_lines(
    text: &str,
    source: &std::path::Path,
    security: &crate::security::ContentSecurity,
) -> Option<Vec<String>> {
    let (keyed, _) = crate::security::private_key_blocks_hidden(text);
    let safe = security.sanitize_text(&keyed, Some(source)).content;
    let lines = safe
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect::<Vec<_>>();
    (lines.len() == text.split('\n').count()).then_some(lines)
}

/// What the judge sees of one hit: which line it is about, and the lines that
/// inform it: its context window, or for a matchOnly span the whole source
/// line or lines holding it, as the content view would show them. `None` when
/// a matchOnly hit's lines cannot be shown, so the hit stays unknown rather
/// than judged from its span alone.
fn hit_state(
    path: &str,
    hit: &TextSearchMatch,
    context: SearchContext,
    match_only: bool,
    evidence: &FileEvidence,
) -> Option<Value> {
    let last = hit.last_line.filter(|&last| last > hit.line);
    let window = if match_only {
        evidence.span(hit.line, last.unwrap_or(hit.line))?
    } else {
        window_lines(hit, context)
    };
    let text = window
        .iter()
        .find(|(line, _)| *line == hit.line)
        .map_or(hit.value.as_str(), |(_, text)| *text);
    let mut target = json!({"line":hit.line, "text":text});
    if let Some(last) = last {
        target["lastLine"] = json!(last);
    }
    if match_only {
        target["match"] = json!(hit.value);
    }
    if let Some(enclosing) = evidence
        .outline
        .as_ref()
        .and_then(|outline| outline.label(outline.owner(hit.line, hit.byte_offset)?))
    {
        target["enclosing"] = json!({"kind":enclosing.kind,"symbolName":enclosing.symbol_name});
    }
    if let Some(key) = evidence.keys.get(&hit.line) {
        target["key"] = json!(key);
    }
    let lines = window
        .iter()
        .map(|(line, text)| json!([line, text]))
        .collect::<Vec<_>>();
    Some(json!({"path":path, "target":target, "lines":lines}))
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
        // names) and its key path (the same `matchKeys` a shown row names), so
        // a predicate about scope (test or production code, which function,
        // which section) can be judged from the candidate itself.
        let evidence = verified.then(|| {
            let lines = hits
                .iter()
                .filter_map(|&index| usize::try_from(file.matches[index].line).ok())
                .collect::<Vec<_>>();
            FileEvidence::read(scope, &file.path, &lines, match_only)
        });
        for &index in hits {
            let hit = &file.matches[index];
            let state = evidence
                .as_ref()
                .and_then(|evidence| hit_state(&file.path, hit, context, match_only, evidence));
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
            ToolId::LocalSearch,
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
    // Unjudged hits are unknown: the undecided view reads them as they are,
    // the next candidate page judges them.
    let (mut rows_total, mut rows_before, mut lines_removed) = (0usize, 0usize, 0usize);
    let files_before = parsed
        .files
        .iter()
        .filter(|file| !file.matches.is_empty())
        .count();
    let mut kept = Vec::with_capacity(parsed.files.len());
    let mut ordinal = 0usize;
    let mut files_dropped = 0usize;
    for (mut file, hits) in std::mem::take(&mut parsed.files).into_iter().zip(file_hits) {
        rows_total += file.matches.len();
        let lines_before = distinct_lines(&file);
        let had_hits = !file.matches.is_empty();
        let mut keep = vec![false; file.matches.len()];
        let mut judged_all = true;
        for index in hits {
            // A hit outside this candidate page's pool was judged (and shown,
            // or left to its unfiltered route) by an earlier one.
            let judged = outcome.considers(ordinal) && !unjudged.contains(&ordinal);
            judged_all &= judged;
            if judged {
                rows_before += 1;
                match decisions[ordinal] {
                    Decision::Selected => keep[index] = true,
                    // Unjudgeable here: shown as found.
                    Decision::Unknown if unavailable => keep[index] = true,
                    Decision::Unknown | Decision::Related | Decision::Excluded => {}
                }
            }
            ordinal += 1;
        }
        let mut flags = keep.into_iter();
        file.matches.retain(|_| flags.next().unwrap_or(false));
        lines_removed += lines_before - distinct_lines(&file);
        if had_hits && file.matches.is_empty() && judged_all {
            files_dropped += 1;
        }
        if !file.matches.is_empty() {
            file.match_count = saturating_u32(file.matches.len());
            kept.push(file);
        }
    }
    parsed.files = kept;
    let rows_kept = parsed.files.iter().map(|file| file.matches.len()).sum();
    let files_kept = parsed.files.len();
    recount(
        query,
        &mut parsed.stats,
        rows_total - rows_kept,
        (lines_removed, files_before - files_kept),
    );
    Ok(Some(Selected {
        outcome,
        rows_before,
        rows_kept,
        files_before,
        files_dropped,
        provider,
    }))
}
