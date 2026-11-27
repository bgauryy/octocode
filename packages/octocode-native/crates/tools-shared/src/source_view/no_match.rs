//! A read whose `matchString` matched nothing: the result, its hints and the
//! repairs that run.

use super::prepare::*;
use super::types::*;
use crate::source_view::local_fetch_query;
use octocode_contract_types::{CaseMode, LocalFetchQuery};
use std::path::Path;

/// What a miss over text holding `masked` masked secret values leaves out,
/// as a clause for its one hint: text a read masks is matched by no search,
/// so the miss proves no absence inside it. The one wording of every tool
/// that searches redacted text.
#[must_use]
pub fn masked_unsearched(masked: usize) -> String {
    let (values, verb) = if masked == 1 {
        ("1 masked secret value".to_owned(), "is")
    } else {
        (format!("{masked} masked secret values"), "are")
    };
    format!("{values} (maskedSpans) {verb} never searched")
}

/// A matchString that selected no line: an empty row with its one hint and
/// at most one call, the first cause that explains the miss. In order: the
/// same read ignoring case, when that selects lines; the same read without
/// its `ranges`, when the file has hits outside them; the read of the names
/// a misremembered declaration head declares, when they select lines; the
/// file's outline, which lists what it declares; else the search of its
/// directory. The first three are counted in the file, so their hint states
/// a fact; the last two are where to look next, and claim absence only for
/// the text searched: over masked secret values they say those were not
/// searched ([`masked_unsearched`]).
pub(super) fn no_match(q: &LocalFetchQuery, source: &Source<'_>) -> LocalFetchResult {
    // Every lead below counts lines of the redacted text, so it is the same
    // for a right and a wrong guess at a secret.
    let written = source.written();
    let lines = |count: usize| {
        if count == 1 {
            "1 line matches".to_owned()
        } else {
            format!("{count} lines match")
        }
    };
    let ignoring_case = || {
        let count = insensitive_match_count(q, &source.text, &written);
        (count > 0).then(|| {
            let mut query = q.clone();
            if let Some(matched) = local_fetch_query::matched_mut(&mut query) {
                matched.case_mode = CaseMode::Insensitive;
            }
            let hint = format!(
                "No case-sensitive match; {} ignoring case: the related[] call reads them.",
                lines(count)
            );
            (
                hint,
                NextCalls {
                    ignore_case: Some(FetchCall::related(query)),
                    ..NextCalls::default()
                },
            )
        })
    };
    let outside_ranges = || {
        if !local_fetch_query::has_ranges(q) {
            return None;
        }
        let whole = local_fetch_query::updated(q, serde_json::json!({"ranges": null})).ok()?;
        let count = super::extraction::extract(&whole, &source.text, &written)
            .ok()
            .and_then(|extraction| extraction.count)
            .filter(|count| *count > 0)?;
        let hint = format!(
            "No match in the requested ranges; {} outside them: the related[] call reads them.",
            lines(count)
        );
        Some((
            hint,
            NextCalls {
                read: Some(FetchCall::related(whole)),
                ..NextCalls::default()
            },
        ))
    };
    // A declaration head misremembered (`pub fn apply(` where the file
    // declares `pub(crate) fn apply`) is read by its name alone.
    let named = || {
        let (count, names, query) = named_read(q, &source.text, &written)?;
        let hint = format!(
            "No line holds it as written; {} `{names}`: the related[] call reads them.",
            lines(count)
        );
        Some((
            hint,
            NextCalls {
                read: Some(FetchCall::related(query)),
                ..NextCalls::default()
            },
        ))
    };
    // What the miss proves: nothing of this file's text, or, when the file
    // holds masked values, nothing of the text outside them.
    let masked = source.redactions_total();
    let absent = if masked == 0 {
        "No line of this file holds it".to_owned()
    } else {
        format!(
            "No line of the searched text holds it; {}",
            masked_unsearched(masked)
        )
    };
    let outline = || {
        let path = local_fetch_query::path(q);
        let read = outlined(path)
            .then(|| local_fetch_query::outline_lead(path))
            .flatten()?;
        Some((
            format!("{absent}; the related[] call lists what the file declares."),
            NextCalls {
                read: Some(FetchCall::related(read)),
                ..NextCalls::default()
            },
        ))
    };
    let elsewhere = || {
        let dir = Path::new(local_fetch_query::path(q))
            .parent()
            .map(|dir| dir.to_string_lossy().into_owned())
            .filter(|dir| !dir.is_empty())
            .unwrap_or_else(|| ".".to_owned());
        let search = super::extraction::search_lead(q, &dir, &local_fetch_query::match_strings(q))?;
        Some((
            format!("{absent}; the related[] call searches its directory."),
            NextCalls {
                text_search: Some(search),
                ..NextCalls::default()
            },
        ))
    };
    let (hint, next) = ignoring_case()
        .or_else(outside_ranges)
        .or_else(named)
        .or_else(outline)
        .or_else(elsewhere)
        .map_or_else(
            || (format!("{absent}."), None),
            |(hint, next)| (hint, Some(next)),
        );
    LocalFetchResult {
        path: local_fetch_query::path(q).to_owned(),
        source_sha256: Some(source.sha256.clone()),
        content: Some(String::new()),
        view: None,
        next,
        hints: vec![hint],
        total_lines: Some(source.total_lines),
        selected_match_count: Some(0),
        masked_spans: Some(masked).filter(|spans| *spans > 0),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        ..LocalFetchResult::blank(ReadStatus::Empty)
    }
}

/// The read a literal read that selected nothing stands for when its
/// patterns are declaration heads misremembered (`fn add_hints(` where the
/// file declares `pub fn add_hints<I>(`) and the file holds exactly one line
/// declaring a name they declare: the same read of that line's own text,
/// with the warning that says it was served and names only the names that
/// line declares, when that read selects that line alone.
/// `None` when a case-insensitive or
/// unranged read would select lines (its own lead says so), no pattern is a
/// head, or no line, or several, declare the names: the empty row's leads
/// then stand.
pub(super) fn declared_read(
    q: &LocalFetchQuery,
    text: &str,
    written: &[std::ops::Range<usize>],
) -> Option<(LocalFetchQuery, String)> {
    // The name read first: it rejects a regex or a pattern that is no head
    // without running a pattern.
    let (_, names, _) = named_read(q, text, written)?;
    if local_fetch_query::has_ranges(q) || insensitive_match_count(q, text, written) > 0 {
        return None;
    }
    let names = names.split("`, `").collect::<Vec<_>>();
    let mut declaring = text.lines().enumerate().filter(|(_, line)| {
        names
            .iter()
            .any(|name| octocode_engine::search::declares_name(line, name))
    });
    let (index, line) = declaring.next()?;
    if declaring.next().is_some() {
        return None;
    }
    // A pattern that is no head stands for itself among `names`: only the
    // names this line declares are what was served.
    let declared = names
        .iter()
        .filter(|name| octocode_engine::search::declares_name(line, name))
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let line = line.trim();
    let query = local_fetch_query::updated(q, serde_json::json!({ "matchString": [line] })).ok()?;
    // The read selects that one line, or the lead stands: the same text on
    // another line would make it a read of more than the declaration.
    super::extraction::extract(&query, text, written)
        .ok()
        .and_then(|extraction| extraction.count)
        .filter(|count| *count == 1)?;
    let patterns = local_fetch_query::match_strings(q)
        .iter()
        .map(|pattern| format!("`{pattern}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let warning = format!(
        "No line holds {patterns}: served the declaration of {declared} at line {} instead.",
        index + 1
    );
    Some((query, warning))
}

/// The same read of the names a literal read's declaration heads declare
/// (`pub fn apply(` names `apply`), each as a whole identifier, with the
/// lines it selects and the names
/// joined; `None` when no pattern is a head, the read is a regex, or the
/// names select no line.
fn named_read(
    q: &LocalFetchQuery,
    text: &str,
    written: &[std::ops::Range<usize>],
) -> Option<(usize, String, LocalFetchQuery)> {
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
    // Each name matches as a whole identifier (the readers take no
    // `wholeWord`): `emitt` is not a use of the name inside `EventEmitter`.
    let whole = |name: &str| {
        let edge = |c: Option<char>| {
            if c.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                r"\b"
            } else {
                ""
            }
        };
        format!(
            "{}{}{}",
            edge(name.chars().next()),
            regex::escape(name),
            edge(name.chars().next_back())
        )
    };
    let query = local_fetch_query::updated(
        q,
        serde_json::json!({
            "matchString": names.iter().map(|name| whole(name)).collect::<Vec<_>>(),
            "regexEngine": "rust",
        }),
    )
    .ok()?;
    let count = super::extraction::extract(&query, text, written)
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
fn insensitive_match_count(
    q: &LocalFetchQuery,
    text: &str,
    written: &[std::ops::Range<usize>],
) -> usize {
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
    super::extraction::extract(&insensitive, text, written)
        .ok()
        .and_then(|extraction| extraction.count)
        .unwrap_or(0)
}
