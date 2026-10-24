//! A read whose `matchString` matched nothing: the result, its hints and the
//! repairs that run.

use super::prepare::*;
use super::types::*;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::{CaseMode, LocalFetchQuery};
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
        if let Some(matched) = local_fetch_query::matched_mut(&mut query) {
            matched.case_mode = CaseMode::Insensitive;
        }
        FetchCall::related(query)
    });
    // A `ranges` read that missed while this file has hits outside its
    // ranges: the same read without them selects those lines.
    let outside = local_fetch_query::has_ranges(q)
        .then(|| {
            let whole = local_fetch_query::updated(q, serde_json::json!({"ranges": null})).ok()?;
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
        FetchCall::related(query)
    });
    let search = read
        .is_none()
        .then(|| {
            let dir = Path::new(local_fetch_query::path(q))
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .filter(|dir| !dir.is_empty())
                .unwrap_or_else(|| ".".to_owned());
            super::extraction::search_lead(q, &dir, &local_fetch_query::match_strings(q))
        })
        .flatten();
    LocalFetchResult {
        path: local_fetch_query::path(q).to_owned(),
        source_sha256: Some(source.sha256.clone()),
        content: Some(String::new()),
        view: None,
        next: Some(NextCalls {
            text_search: search,
            ignore_case,
            read,
            ..NextCalls::default()
        })
        .filter(|next| !next.is_empty()),
        warnings,
        hints: redacted
            .then(|| REDACTED_MATCH_HINT.to_owned())
            .into_iter()
            .collect(),
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
pub(crate) const REDACTED_MATCH_HINT: &str = "matchString ran on redacted text: text behind this file's [REDACTED…] placeholders cannot match.";

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
    if let Some(matched) = local_fetch_query::matched_mut(&mut insensitive) {
        matched.case_mode = CaseMode::Insensitive;
    }
    super::extraction::extract(&insensitive, text)
        .ok()
        .and_then(|extraction| extraction.count)
        .unwrap_or(0)
}
