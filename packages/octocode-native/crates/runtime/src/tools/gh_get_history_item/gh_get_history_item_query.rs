//! Operation-independent views over the generated wire query, in the
//! engine's `usize` units.
use super::{DEFAULT_PAGE_SIZE, GhGetHistoryItemQuery, ItemOperation};
use crate::num::usize_of;
/// Any `sections` named: the read returns what they name.
pub(crate) fn has_sections(query: &GhGetHistoryItemQuery) -> bool {
    match query {
        GhGetHistoryItemQuery::PullRequest { sections, .. } => !sections.is_empty(),
        GhGetHistoryItemQuery::Issue { sections, .. } => !sections.is_empty(),
        GhGetHistoryItemQuery::Commit { sections, .. } => !sections.is_empty(),
        GhGetHistoryItemQuery::Compare { sections, .. } => !sections.is_empty(),
    }
}
pub(super) fn operation(query: &GhGetHistoryItemQuery) -> ItemOperation {
    match query {
        GhGetHistoryItemQuery::PullRequest { .. } => ItemOperation::PullRequest,
        GhGetHistoryItemQuery::Issue { .. } => ItemOperation::Issue,
        GhGetHistoryItemQuery::Commit { .. } => ItemOperation::Commit,
        GhGetHistoryItemQuery::Compare { .. } => ItemOperation::Compare,
    }
}
pub(crate) fn owner(query: &GhGetHistoryItemQuery) -> &str {
    match query {
        GhGetHistoryItemQuery::PullRequest { owner, .. }
        | GhGetHistoryItemQuery::Issue { owner, .. }
        | GhGetHistoryItemQuery::Commit { owner, .. }
        | GhGetHistoryItemQuery::Compare { owner, .. } => owner.as_str(),
    }
}
pub(crate) fn repo(query: &GhGetHistoryItemQuery) -> &str {
    match query {
        GhGetHistoryItemQuery::PullRequest { repo, .. }
        | GhGetHistoryItemQuery::Issue { repo, .. }
        | GhGetHistoryItemQuery::Commit { repo, .. }
        | GhGetHistoryItemQuery::Compare { repo, .. } => repo.as_str(),
    }
}
pub(crate) fn number(query: &GhGetHistoryItemQuery) -> Option<u64> {
    match query {
        GhGetHistoryItemQuery::PullRequest { number, .. }
        | GhGetHistoryItemQuery::Issue { number, .. } => Some(number.0.get()),
        _ => None,
    }
}
pub(crate) fn reference(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::Commit { ref_, .. } => Some(ref_.as_str()),
        _ => None,
    }
}
pub(crate) fn base(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::Compare { base, .. } => Some(base.as_str()),
        _ => None,
    }
}
pub(crate) fn head(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::Compare { head, .. } => Some(head.as_str()),
        _ => None,
    }
}
/// The `include` path patterns (pull request, commit, comparison).
pub(crate) fn include_patterns(query: &GhGetHistoryItemQuery) -> Vec<&str> {
    match query {
        GhGetHistoryItemQuery::PullRequest { include, .. }
        | GhGetHistoryItemQuery::Commit { include, .. }
        | GhGetHistoryItemQuery::Compare { include, .. } => include
            .iter()
            .flat_map(|include| include.iter())
            .map(|pattern| pattern.as_str())
            .collect(),
        GhGetHistoryItemQuery::Issue { .. } => Vec::new(),
    }
}
/// The pull-request head a page continuation is pinned to (`headSha`).
pub(crate) fn head_sha(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::PullRequest { head_sha, .. } => {
            head_sha.as_deref().map(String::as_str)
        }
        _ => None,
    }
}
/// The page of the section a read lists (`page`): every page
/// continuation names the one section it pages.
pub(crate) fn page(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest { page, .. }
        | GhGetHistoryItemQuery::Issue { page, .. }
        | GhGetHistoryItemQuery::Commit { page, .. }
        | GhGetHistoryItemQuery::Compare { page, .. } => {
            page.as_ref().map(|page| usize_of(page.0.get()))
        }
    }
}
pub(crate) fn page_size(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest { page_size, .. }
        | GhGetHistoryItemQuery::Commit { page_size, .. }
        | GhGetHistoryItemQuery::Compare { page_size, .. } => {
            page_size.map(|size| usize_of(size.get()))
        }
        GhGetHistoryItemQuery::Issue { page_size, .. } => {
            page_size.as_ref().map(|size| usize_of(size.0.get()))
        }
    }
}
/// Items per page of a provider collection (comments, reviews, commits,
/// patch file pages): `pageSize` capped at one provider batch.
pub(crate) fn collection_page_size(query: &GhGetHistoryItemQuery) -> usize {
    page_size(query)
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, octocode_github::MAX_PER_PAGE)
}
/// Whether a pull request narrows its changed files (`include`,
/// `status`, `minChanges`).
pub(crate) fn has_file_filter(query: &GhGetHistoryItemQuery) -> bool {
    matches!(
        query,
        GhGetHistoryItemQuery::PullRequest { include, status, min_changes, .. }
            if include.is_some() || status.is_some() || min_changes.is_some()
    )
}

/// The text window offset (`offset`, characters).
pub(crate) fn char_offset(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest { offset, .. }
        | GhGetHistoryItemQuery::Issue { offset, .. }
        | GhGetHistoryItemQuery::Commit { offset, .. }
        | GhGetHistoryItemQuery::Compare { offset, .. } => {
            offset.as_ref().map(|offset| usize_of(offset.0))
        }
    }
}
/// The text window length (`length`, characters).
pub(crate) fn char_length(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest { length, .. }
        | GhGetHistoryItemQuery::Issue { length, .. }
        | GhGetHistoryItemQuery::Commit { length, .. }
        | GhGetHistoryItemQuery::Compare { length, .. } => {
            length.as_ref().map(|length| usize_of(length.0.get()))
        }
    }
}
pub(crate) fn match_string(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::PullRequest { match_string, .. }
        | GhGetHistoryItemQuery::Commit { match_string, .. }
        | GhGetHistoryItemQuery::Compare { match_string, .. } => {
            match_string.as_ref().map(|text| text.0.as_str())
        }
        GhGetHistoryItemQuery::Issue { .. } => None,
    }
}
/// Lines kept around each `matchString` hit in a patch (`contextLines`).
pub(crate) fn match_context(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest { context_lines, .. }
        | GhGetHistoryItemQuery::Commit { context_lines, .. }
        | GhGetHistoryItemQuery::Compare { context_lines, .. } => context_lines
            .as_ref()
            .and_then(|lines| usize::try_from(lines.0).ok()),
        GhGetHistoryItemQuery::Issue { .. } => None,
    }
}
/// Whether the query reads a later page of a pull request (a section
/// page after the first, or a text window that names its offset): the
/// caller already holds the item header and menu from page one.
pub(crate) fn later_page(query: &GhGetHistoryItemQuery) -> bool {
    matches!(query, GhGetHistoryItemQuery::PullRequest { .. })
        && (page(query).is_some_and(|page| page > 1) || char_offset(query).is_some())
}
/// Whether the read's `extras` names `kind`.
pub(crate) fn wants(
    query: &GhGetHistoryItemQuery,
    kind: octocode_contract_types::ExtraKind,
) -> bool {
    match query {
        GhGetHistoryItemQuery::PullRequest { extras, .. }
        | GhGetHistoryItemQuery::Issue { extras, .. }
        | GhGetHistoryItemQuery::Commit { extras, .. }
        | GhGetHistoryItemQuery::Compare { extras, .. } => extras.contains(&kind),
    }
}
