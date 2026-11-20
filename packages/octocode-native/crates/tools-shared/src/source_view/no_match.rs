//! A read whose `matchString` matched nothing: the result, its hints and the
//! repairs that run.

use super::prepare::*;
use super::types::*;
use crate::source_view::local_fetch_query;
use octocode_contract_types::{CaseMode, LocalFetchQuery};
use std::path::Path;

/// A matchString that selected no line: an empty row with its facts.
pub(super) fn no_match(q: &LocalFetchQuery, source: &Source<'_>) -> LocalFetchResult {
    // Every lead below counts lines of the redacted text, so it is the same
    // for a right and a wrong guess at a secret.
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
    // The folder search is for a pattern this file does not hold at all.
    let elsewhere = read.is_none();
    // Nothing else in this file matched: the file itself is read again
    // before any folder is searched (its lead comes first). A declaration
    // head misremembered (`pub fn apply(` where the file declares
    // `pub(crate) fn apply`) is read by its name alone when that selects
    // lines; else the file's outline lists what it declares.
    let read = read.or_else(|| {
        if ignore_case.is_some() {
            return None;
        }
        if let Some((count, names, query)) = named_read(q, &source.text) {
            warnings.push(format!(
                "{count} {} `{names}`.",
                if count == 1 {
                    "line matches"
                } else {
                    "lines match"
                }
            ));
            return Some(FetchCall::related(query));
        }
        outlined(local_fetch_query::path(q))
            .then(|| local_fetch_query::outline_lead(local_fetch_query::path(q)))
            .flatten()
            .map(FetchCall::related)
    });
    let search = elsewhere
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
        total_lines: Some(source.total_lines),
        selected_match_count: Some(0),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        ..LocalFetchResult::blank(ReadStatus::Empty)
    }
}

/// The same read of the names a literal read's declaration heads declare
/// (`pub fn apply(` names `apply`), with the lines it selects and the names
/// joined; `None` when no pattern is a head, the read is a regex, or the
/// names select no line.
fn named_read(q: &LocalFetchQuery, text: &str) -> Option<(usize, String, LocalFetchQuery)> {
    if local_fetch_query::is_regex(q) {
        return None;
    }
    let patterns = local_fetch_query::match_strings(q);
    let mut names = Vec::with_capacity(patterns.len());
    for pattern in &patterns {
        let name = octocode_engine::search::declared_name(pattern).unwrap_or(pattern);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.iter().all(|name| patterns.contains(name)) {
        return None;
    }
    let query = local_fetch_query::updated(
        q,
        serde_json::json!({ "matchString": names, "wholeWord": true }),
    )
    .ok()?;
    let count = super::extraction::extract(&query, text)
        .ok()
        .and_then(|extraction| extraction.count)
        .filter(|count| *count > 0)?;
    Some((count, names.join("`, `"), query))
}

/// Whether `path`'s language has a declaration outline (`view:"symbols"`).
fn outlined(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            octocode_engine::signatures::languages::find_entry(&extension.to_ascii_lowercase())
                .is_some()
        })
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
    if let Some(matched) = local_fetch_query::matched_mut(&mut insensitive) {
        matched.case_mode = CaseMode::Insensitive;
    }
    super::extraction::extract(&insensitive, text)
        .ok()
        .and_then(|extraction| extraction.count)
        .unwrap_or(0)
}
