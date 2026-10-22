//! A read whose `matchString` matched nothing: the result, its hints and the
//! repairs that run.

use super::prepare::*;
use super::types::*;
use std::path::Path;

/// A matchString that selected no line: an empty row with its facts.
pub(super) fn no_match(q: &LocalFetchQuery, source: &Source) -> LocalFetchResult {
    // In a file with redactions the text may sit behind a placeholder; that
    // fact depends only on the file, never on the guess (no oracle), and
    // reveals no more than a plain fetch (which shows the placeholders).
    // Every lead below counts lines of the redacted text, so it is the same
    // for a right and a wrong guess at a secret: the leads stay, and the
    // redaction is one more fact beside them.
    let redacted = source.match_redacted || source.key_blocks_redacted;
    let mut warnings = Vec::new();
    // A case-sensitive miss reruns as the same read, case-insensitive, only
    // when that read selects lines; the count is the fact beside the lead.
    let insensitive = Some(insensitive_match_count(q, &source.text)).filter(|count| *count > 0);
    let ignore_case = insensitive.map(|count| {
        warnings.push(format!(
            "{count} {} ignoring case.",
            if count == 1 {
                "line matches"
            } else {
                "lines match"
            }
        ));
        let mut query = q.clone();
        query.case_mode = Some(ReadCaseMode::Insensitive);
        FetchCall { query }
    });
    // A `ranges` read that missed while this file has hits outside its
    // ranges: the same read without them selects those lines.
    let outside = local_fetch_query::has_ranges(q)
        .then(|| {
            let mut whole = q.clone();
            whole.ranges.clear();
            let count = super::extraction::extract(&whole, &source.text)
                .ok()
                .and_then(|extraction| extraction.count)
                .unwrap_or(0);
            (count > 0).then_some((count, whole))
        })
        .flatten();
    let read = outside.map(|(count, query)| {
        warnings.push(format!(
            "{count} {} outside the requested ranges.",
            if count == 1 {
                "line matches"
            } else {
                "lines match"
            }
        ));
        FetchCall { query }
    });
    // A literal `|` read as alternation: the same read with each side as
    // its own matchString, when that selects lines here.
    let alternatives = bar_alternatives(q);
    let repair = alternatives
        .as_ref()
        .filter(|_| read.is_none())
        .and_then(|query| {
            let count = super::extraction::extract(query, &source.text)
                .ok()
                .and_then(|extraction| extraction.count)
                .unwrap_or(0);
            (count > 0).then(|| {
                warnings.push(format!(
                    "{count} {} with each `|`-separated part as its own matchString.",
                    if count == 1 {
                        "line matches"
                    } else {
                        "lines match"
                    }
                ));
                FetchCall {
                    query: query.clone(),
                }
            })
        });
    // The text may live in another file of this directory: one localSearch
    // there finds it (not offered when this file holds it outside the
    // ranges); a literal `|` searches for each of its parts.
    let search = (read.is_none() && repair.is_none())
        .then(|| {
            let dir = Path::new(q.path.as_str())
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .filter(|dir| !dir.is_empty())
                .unwrap_or_else(|| ".".to_owned());
            let searched = alternatives.as_ref().unwrap_or(q);
            super::extraction::search_lead(
                searched,
                &dir,
                &local_fetch_query::match_strings(searched),
            )
        })
        .flatten();
    LocalFetchResult {
        path: q.path.to_string(),
        source_sha256: Some(source.sha256.clone()),
        content: Some(String::new()),
        content_view: None,
        next: Some(NextCalls {
            text_search: search,
            ignore_case,
            read,
            repair,
            ..NextCalls::default()
        })
        .filter(|next| !next.is_empty()),
        warnings,
        hints: no_match_hints(q, redacted),
        total_lines: Some(source.total_lines),
        selected_match_count: Some(0),
        source_chars: Some(source.chars),
        source_bytes: Some(source.bytes),
        returned_chars: Some(0),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        ..LocalFetchResult::blank(ReadStatus::Empty)
    }
}

/// Why a matchString can miss text that is in the file: it ran on the
/// redacted text, whose secrets are placeholders. A fact of the file, stated
/// beside the miss's leads, not as its cause.
pub(super) const REDACTED_MATCH_HINT: &str = "matchString ran on redacted text: text behind this file's [REDACTED…] placeholders cannot match.";

/// The per-call facts of a matchString that selected no line: redaction
/// (the text may sit behind a placeholder) and a literal `|` (read as
/// alternation by most agents; it is not one). The leads beside the row
/// (`ignoreCase`, `findText`) are its next steps.
fn no_match_hints(q: &LocalFetchQuery, redacted: bool) -> Vec<String> {
    let mut hints = Vec::new();
    if redacted {
        hints.push(REDACTED_MATCH_HINT.to_owned());
    }
    if !local_fetch_query::is_regex(q)
        && local_fetch_query::match_strings(q)
            .iter()
            .any(|pattern| pattern.contains('|'))
    {
        hints.push("`|` in matchString matched literally, not as alternation.".to_owned());
    }
    hints
}

/// A literal read whose matchString holds `|`: the same read with every
/// `|`-separated part as its own matchString (a list matches any entry),
/// as alternation would; `None` without a `|` or past the list limit.
fn bar_alternatives(q: &LocalFetchQuery) -> Option<LocalFetchQuery> {
    use crate::contracts::query_limits::local_fetch::MATCH_STRING_MAX_ITEMS;
    let patterns = local_fetch_query::match_strings(q);
    if local_fetch_query::is_regex(q) || !patterns.iter().any(|pattern| pattern.contains('|')) {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in patterns.iter().flat_map(|pattern| pattern.split('|')) {
        if !part.is_empty() && !parts.contains(&part) {
            parts.push(part);
        }
    }
    if parts.is_empty() || parts.len() > MATCH_STRING_MAX_ITEMS {
        return None;
    }
    let mut query = q.clone();
    query.match_string = serde_json::from_value(serde_json::json!(parts)).ok();
    query.match_string.is_some().then_some(query)
}

/// Lines the same read selects when every pattern matches case-insensitively;
/// 0 when the patterns do not compile that way either, or already matched so.
fn insensitive_match_count(q: &LocalFetchQuery, text: &str) -> usize {
    // A read that matched every pattern ignoring case already ran this one.
    if !local_fetch_query::match_strings(q)
        .iter()
        .any(|pattern| local_fetch_query::case_sensitive_for(q, pattern))
    {
        return 0;
    }
    let mut insensitive = q.clone();
    insensitive.case_mode = Some(ReadCaseMode::Insensitive);
    super::extraction::extract(&insensitive, text)
        .ok()
        .and_then(|extraction| extraction.count)
        .unwrap_or(0)
}
