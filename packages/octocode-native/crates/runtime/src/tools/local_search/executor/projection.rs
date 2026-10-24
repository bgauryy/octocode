//! The page's rows: files ordered, shown rows projected, and each hit
//! annotated with its match keys and enclosing declaration.
use crate::tools::local_search::local_search_query::View;
use octocode_contract_types::CaseMode;
use octocode_contract_types::RegexEngine;
use octocode_engine::types::SearchSort;

use super::super::enclosing::{annotate_enclosing, annotate_match_keys};
use super::super::handoff::Definition;
use super::super::layout::{Layout, file_pagination};
use super::super::rows::{merge_context_windows, project_match};
use super::super::types::SearchFile;
use super::super::verify::{Outlines, PageScope, Redacted};

use super::super::local_search_query::{self, effective_match_content_length};
use crate::execution::cancel::CancellationCheck;
use octocode_contract_types::LocalSearchQuery;

/// The page's rows, projected and annotated: each hit's match keys and
/// enclosing declaration, and the searched symbol's declaration among them.
pub(super) struct Annotated<'q> {
    pub(super) files: Vec<SearchFile>,
    /// Shown values redaction changed.
    pub(super) shown_redacted: usize,
    pub(super) symbol: Option<&'q str>,
    pub(super) definition: Option<Definition>,
    /// Warnings the annotations add.
    pub(super) notes: Vec<String>,
}

/// Project the page's shown rows and annotate them (see [`Annotated`]).
pub(super) fn annotate_rows<'q>(
    scope: &PageScope<'q, impl CancellationCheck>,
    layout: &Layout,
    scanned: Vec<octocode_engine::types::TextSearchFile>,
    redacted: &Redacted,
    outlines: Outlines,
    page_budget: usize,
) -> Annotated<'q> {
    let PageScope {
        query,
        output_root,
        expected_digest,
        security,
        ..
    } = *scope;
    let (mut files, shown_redacted) = project_files(query, layout, scanned, redacted);
    let symbol = searched_symbol(query, layout);
    // Rows render with the root's prefix, which the page cut charged.
    let keys_budget = page_budget.saturating_sub(layout.prefix_chars.saturating_mul(files.len()));
    let mut notes = Vec::new();
    notes.extend(annotate_match_keys(
        &mut files,
        output_root,
        keys_budget,
        expected_digest,
        security,
    ));
    let (definition, enclosing_note) = annotate_enclosing(
        &mut files,
        symbol,
        symbol.filter(|_| {
            super::super::enclosing::declaration_search(local_search_query::pattern(query))
        }),
        output_root,
        expected_digest,
        outlines,
        security,
    );
    notes.extend(enclosing_note);
    Annotated {
        files,
        shown_redacted,
        symbol,
        definition,
        notes,
    }
}

/// The symbol a page names declarations for: a small page names each hit's
/// enclosing declaration, and a searched symbol declared on a shown hit
/// line gets an lspSearch lead.
fn searched_symbol<'q>(query: &'q LocalSearchQuery, layout: &Layout) -> Option<&'q str> {
    (!layout.list
        && local_search_query::case_mode(query) != CaseMode::Insensitive
        && local_search_query::regex_mode(query) != RegexEngine::Pcre2
        && local_search_query::invert_match(query) != Some(true))
    .then(|| super::super::enclosing::searched_symbol(local_search_query::pattern(query)))
    .flatten()
}

fn is_count_view(query: &LocalSearchQuery) -> bool {
    matches!(
        local_search_query::view(query),
        View::CountMatches | View::CountLines
    )
}

/// The runtime's own orders (path, matchCount), then `reverse`. Engine-side
/// time and relevance sorts already honour `reverse`; every order the
/// runtime (re)establishes — path, matchCount, traversal — is reversed here,
/// as the schema promises ("after sort, before pagination").
pub(crate) fn order_files(
    query: &LocalSearchQuery,
    files: &mut [octocode_engine::types::TextSearchFile],
) {
    let sort = local_search_query::sort(query);
    match sort {
        SearchSort::Path => files.sort_by(|a, b| a.path.cmp(&b.path)),
        SearchSort::MatchCount => files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.path.cmp(&b.path))
        }),
        // A count view under relevance ranks the most hits first; the stable
        // sort keeps the engine's relevance order (declaration before code,
        // comment, string) among equal counts. Under `reverse` the engine
        // already reversed that order, so the fewest hits come first with
        // ties left in the engine's reversed order: the exact reverse of the
        // forward listing.
        SearchSort::Relevance if is_count_view(query) => {
            if local_search_query::reverse(query).unwrap_or(false) {
                files.sort_by_key(|file| file.match_count);
            } else {
                files.sort_by_key(|file| std::cmp::Reverse(file.match_count));
            }
        }
        _ => {}
    }
    // Relevance (count views included, above) and the time sorts arrive in
    // order with `reverse` applied.
    let engine_sorted = matches!(
        sort,
        SearchSort::Modified | SearchSort::Accessed | SearchSort::Created | SearchSort::Relevance
    );
    if local_search_query::reverse(query).unwrap_or(false) && !engine_sorted {
        files.reverse();
    }
}

/// The page's file entries, and how many shown values were redacted.
fn project_files(
    query: &LocalSearchQuery,
    layout: &Layout,
    scanned: Vec<octocode_engine::types::TextSearchFile>,
    redacted: &Redacted,
) -> (Vec<SearchFile>, usize) {
    let match_only = local_search_query::view(query) == View::MatchOnly;
    let match_page = local_search_query::match_page(query).max(1);
    let mut shown_redacted = 0usize;
    let mut slots = scanned.into_iter().map(Some).collect::<Vec<_>>();
    let files = layout
        .shown
        .iter()
        .filter_map(|(index, rows)| Some((slots.get_mut(*index)?.take()?, rows.clone())))
        .map(|(f, rows)| {
            // One key per file, its line and column set per row.
            let mut key = (f.path.clone(), 0, 0);
            let total = f.matches.len() as u32;
            let end = rows.end.min(f.matches.len());
            let start = rows.start.min(end);
            let mut page_rows = f.matches[start..end].iter().collect::<Vec<_>>();
            page_rows.sort_by_key(|m| (m.line, m.column));
            let shown = page_rows
                .into_iter()
                .inspect(|m| {
                    (key.1, key.2) = (m.line, m.column);
                    if redacted.contains(&key) {
                        shown_redacted += 1;
                    }
                })
                .map(|m| project_match(m, layout.display_cap))
                .map(|mut row| {
                    if !match_only {
                        row.column = None;
                    }
                    row
                })
                .collect::<Vec<_>>();
            // Rows on later pages, named by line so a reader can fetch them
            // directly instead of paging.
            let mut later = f.matches[end..].iter().map(|m| m.line).collect::<Vec<_>>();
            later.sort_unstable();
            later.dedup();
            let shown = match layout.merge_context {
                Some(context) => {
                    let mut hits = f.matches.iter().map(|m| m.line).collect::<Vec<_>>();
                    hits.sort_unstable();
                    hits.dedup();
                    merge_context_windows(
                        shown,
                        context,
                        effective_match_content_length(query) as usize,
                        &hits,
                    )
                }
                None => shown,
            };
            let span = (start, end, shown.len());
            SearchFile {
                matches: (!layout.list).then_some(shown),
                total_occurrences: (local_search_query::view(query) == View::CountMatches)
                    .then_some(f.match_count),
                total_matched_lines: (local_search_query::view(query) == View::CountLines)
                    .then_some(f.match_count),
                pagination: file_pagination(layout, total, span, &later, match_page),
                path: f.path,
                match_keys: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    // A later match page must not re-send files whose rows ended on an
    // earlier page as empty `outOfRange` rows. Keep them only when every
    // file on this page is exhausted, so a forged/stale matchPage still gets
    // its out-of-range diagnostic instead of a silent empty page.
    let exhausted = |file: &SearchFile| file.pagination.as_ref().is_some_and(|p| p.out_of_range);
    let mut files = files;
    if !layout.list && files.iter().any(|file| !exhausted(file)) {
        files.retain(|file| !exhausted(file));
    }
    (files, shown_redacted)
}
