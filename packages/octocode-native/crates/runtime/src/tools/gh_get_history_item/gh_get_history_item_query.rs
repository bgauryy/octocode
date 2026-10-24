use super::{DEFAULT_PAGE_SIZE, GhGetHistoryItemQuery, ItemOperation};
use crate::num::usize_of;
use octocode_contract_types::{CaseMode, RegexEngine, TextMatchTerms};
pub(crate) fn has_sections(query: &GhGetHistoryItemQuery) -> bool {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => !q.sections.is_empty(),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => !q.sections.is_empty(),
        GhGetHistoryItemQuery::Issue(q) => !q.sections.is_empty(),
        GhGetHistoryItemQuery::Commit3(q) => !q.sections.is_empty(),
        GhGetHistoryItemQuery::CommitMatchString(q) => !q.sections.is_empty(),
        GhGetHistoryItemQuery::Compare5(q) => !q.sections.is_empty(),
        GhGetHistoryItemQuery::CompareMatchString(q) => !q.sections.is_empty(),
    }
}
pub(super) fn operation(query: &GhGetHistoryItemQuery) -> ItemOperation {
    match query {
        GhGetHistoryItemQuery::PullRequest0(..) => ItemOperation::PullRequest,
        GhGetHistoryItemQuery::PullRequestMatchString(..) => ItemOperation::PullRequest,
        GhGetHistoryItemQuery::Issue(..) => ItemOperation::Issue,
        GhGetHistoryItemQuery::Commit3(..) => ItemOperation::Commit,
        GhGetHistoryItemQuery::CommitMatchString(..) => ItemOperation::Commit,
        GhGetHistoryItemQuery::Compare5(..) => ItemOperation::Compare,
        GhGetHistoryItemQuery::CompareMatchString(..) => ItemOperation::Compare,
    }
}
pub(crate) fn owner(query: &GhGetHistoryItemQuery) -> &str {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.owner.as_str(),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => q.owner.as_str(),
        GhGetHistoryItemQuery::Issue(q) => q.owner.as_str(),
        GhGetHistoryItemQuery::Commit3(q) => q.owner.as_str(),
        GhGetHistoryItemQuery::CommitMatchString(q) => q.owner.as_str(),
        GhGetHistoryItemQuery::Compare5(q) => q.owner.as_str(),
        GhGetHistoryItemQuery::CompareMatchString(q) => q.owner.as_str(),
    }
}
pub(crate) fn repo(query: &GhGetHistoryItemQuery) -> &str {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.repo.as_str(),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => q.repo.as_str(),
        GhGetHistoryItemQuery::Issue(q) => q.repo.as_str(),
        GhGetHistoryItemQuery::Commit3(q) => q.repo.as_str(),
        GhGetHistoryItemQuery::CommitMatchString(q) => q.repo.as_str(),
        GhGetHistoryItemQuery::Compare5(q) => q.repo.as_str(),
        GhGetHistoryItemQuery::CompareMatchString(q) => q.repo.as_str(),
    }
}
pub(crate) fn number(query: &GhGetHistoryItemQuery) -> Option<u64> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => Some(q.number.get()),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => Some(q.number.get()),
        GhGetHistoryItemQuery::Issue(q) => Some(q.number.get()),
        _ => None,
    }
}
pub(crate) fn reference(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::Commit3(q) => Some(q.ref_.as_str()),
        GhGetHistoryItemQuery::CommitMatchString(q) => Some(q.ref_.as_str()),
        _ => None,
    }
}
pub(crate) fn base(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::Compare5(q) => Some(q.base_ref.as_str()),
        GhGetHistoryItemQuery::CompareMatchString(q) => Some(q.base_ref.as_str()),
        _ => None,
    }
}
pub(crate) fn head(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::Compare5(q) => Some(q.head_ref.as_str()),
        GhGetHistoryItemQuery::CompareMatchString(q) => Some(q.head_ref.as_str()),
        _ => None,
    }
}
pub(crate) fn include_patterns(query: &GhGetHistoryItemQuery) -> Vec<&str> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.include.iter().map(|x| x.as_str()).collect(),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            q.include.iter().map(|x| x.as_str()).collect()
        }
        GhGetHistoryItemQuery::Commit3(q) => q.include.iter().map(|x| x.as_str()).collect(),
        GhGetHistoryItemQuery::CommitMatchString(q) => {
            q.include.iter().map(|x| x.as_str()).collect()
        }
        GhGetHistoryItemQuery::Compare5(q) => q.include.iter().map(|x| x.as_str()).collect(),
        GhGetHistoryItemQuery::CompareMatchString(q) => {
            q.include.iter().map(|x| x.as_str()).collect()
        }
        _ => Vec::new(),
    }
}
pub(crate) fn head_sha(query: &GhGetHistoryItemQuery) -> Option<&str> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.head_sha.as_ref().map(|x| x.as_str()),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => q.head_sha.as_ref().map(|x| x.as_str()),
        _ => None,
    }
}
pub(crate) fn page(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.page.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => q.page.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Issue(q) => q.page.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Commit3(q) => q.page.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::CommitMatchString(q) => q.page.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Compare5(q) => q.page.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::CompareMatchString(q) => q.page.map(|x| usize_of(x.get())),
    }
}
pub(crate) fn char_offset(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.offset.map(usize_of),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => q.offset.map(usize_of),
        GhGetHistoryItemQuery::Issue(q) => q.offset.map(usize_of),
        GhGetHistoryItemQuery::Commit3(q) => q.offset.map(usize_of),
        GhGetHistoryItemQuery::CommitMatchString(q) => q.offset.map(usize_of),
        GhGetHistoryItemQuery::Compare5(q) => q.offset.map(usize_of),
        GhGetHistoryItemQuery::CompareMatchString(q) => q.offset.map(usize_of),
    }
}
pub(crate) fn has_file_filter(query: &GhGetHistoryItemQuery) -> bool {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => {
            !q.include.is_empty() || !q.file_status.is_empty() || q.min_changes.is_some()
        }
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            !q.include.is_empty() || !q.file_status.is_empty() || q.min_changes.is_some()
        }
        GhGetHistoryItemQuery::Commit3(q) => {
            !q.include.is_empty() || !q.file_status.is_empty() || q.min_changes.is_some()
        }
        GhGetHistoryItemQuery::CommitMatchString(q) => {
            !q.include.is_empty() || !q.file_status.is_empty() || q.min_changes.is_some()
        }
        GhGetHistoryItemQuery::Compare5(q) => {
            !q.include.is_empty() || !q.file_status.is_empty() || q.min_changes.is_some()
        }
        GhGetHistoryItemQuery::CompareMatchString(q) => {
            !q.include.is_empty() || !q.file_status.is_empty() || q.min_changes.is_some()
        }
        _ => false,
    }
}
pub(crate) fn file_status(query: &GhGetHistoryItemQuery) -> Vec<String> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => {
            q.file_status.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            q.file_status.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::Commit3(q) => {
            q.file_status.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::CommitMatchString(q) => {
            q.file_status.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::Compare5(q) => {
            q.file_status.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::CompareMatchString(q) => {
            q.file_status.iter().map(ToString::to_string).collect()
        }
        _ => Vec::new(),
    }
}
pub(crate) fn min_changes(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.min_changes.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            q.min_changes.map(|x| usize_of(x.get()))
        }
        GhGetHistoryItemQuery::Commit3(q) => q.min_changes.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::CommitMatchString(q) => q.min_changes.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Compare5(q) => q.min_changes.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::CompareMatchString(q) => q.min_changes.map(|x| usize_of(x.get())),
        _ => None,
    }
}
pub(crate) fn match_strings(query: &GhGetHistoryItemQuery) -> Option<&TextMatchTerms> {
    match query {
        GhGetHistoryItemQuery::PullRequestMatchString(q) => Some(&q.match_string),
        GhGetHistoryItemQuery::CommitMatchString(q) => Some(&q.match_string),
        GhGetHistoryItemQuery::CompareMatchString(q) => Some(&q.match_string),
        _ => None,
    }
}
pub(crate) fn match_context(query: &GhGetHistoryItemQuery) -> (usize, usize) {
    match query {
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            q.context_lines.as_ref().map_or((0, 0), |c| {
                (
                    crate::num::usize_of_signed(c.before),
                    crate::num::usize_of_signed(c.after),
                )
            })
        }
        GhGetHistoryItemQuery::CommitMatchString(q) => {
            q.context_lines.as_ref().map_or((0, 0), |c| {
                (
                    crate::num::usize_of_signed(c.before),
                    crate::num::usize_of_signed(c.after),
                )
            })
        }
        GhGetHistoryItemQuery::CompareMatchString(q) => {
            q.context_lines.as_ref().map_or((0, 0), |c| {
                (
                    crate::num::usize_of_signed(c.before),
                    crate::num::usize_of_signed(c.after),
                )
            })
        }
        _ => (0, 0),
    }
}
pub(crate) fn match_options(
    query: &GhGetHistoryItemQuery,
) -> Option<(CaseMode, RegexEngine, bool)> {
    match query {
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            Some((q.case_mode, q.regex_engine, q.whole_word))
        }
        GhGetHistoryItemQuery::CommitMatchString(q) => {
            Some((q.case_mode, q.regex_engine, q.whole_word))
        }
        GhGetHistoryItemQuery::CompareMatchString(q) => {
            Some((q.case_mode, q.regex_engine, q.whole_word))
        }
        _ => None,
    }
}
pub(crate) fn section_names(query: &GhGetHistoryItemQuery) -> Vec<String> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => {
            q.sections.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::PullRequestMatchString(q) => {
            q.sections.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::Issue(q) => q.sections.iter().map(ToString::to_string).collect(),
        GhGetHistoryItemQuery::Commit3(q) => q.sections.iter().map(ToString::to_string).collect(),
        GhGetHistoryItemQuery::CommitMatchString(q) => {
            q.sections.iter().map(ToString::to_string).collect()
        }
        GhGetHistoryItemQuery::Compare5(q) => q.sections.iter().map(ToString::to_string).collect(),
        GhGetHistoryItemQuery::CompareMatchString(q) => {
            q.sections.iter().map(ToString::to_string).collect()
        }
    }
}
pub(crate) fn collection_page_size(query: &GhGetHistoryItemQuery) -> usize {
    page_size(query)
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(octocode_github::MAX_PER_PAGE)
}
pub(crate) fn later_page(query: &GhGetHistoryItemQuery) -> bool {
    operation(query) == ItemOperation::PullRequest
        && (page(query).is_some_and(|p| p > 1) || char_offset(query).is_some())
}

pub(crate) fn page_size(query: &GhGetHistoryItemQuery) -> Option<usize> {
    match query {
        GhGetHistoryItemQuery::PullRequest0(q) => q.page_size.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::PullRequestMatchString(q) => q.page_size.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Issue(q) => q.page_size.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Commit3(q) => q.page_size.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::CommitMatchString(q) => q.page_size.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::Compare5(q) => q.page_size.map(|x| usize_of(x.get())),
        GhGetHistoryItemQuery::CompareMatchString(q) => q.page_size.map(|x| usize_of(x.get())),
    }
}
