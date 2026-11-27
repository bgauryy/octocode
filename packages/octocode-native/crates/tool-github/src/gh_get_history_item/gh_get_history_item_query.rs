use super::{DEFAULT_PAGE_SIZE, GhGetHistoryItemQuery as Q, ItemOperation};
use octocode_contract_types::{CaseMode, RegexEngine, TextMatchTerms};
use octocode_engine::num::saturating_usize;
use octocode_tools_shared::variant_field::variant_field;

/// Binds `$q` to the query struct of every operation.
macro_rules! any_operation {
    ($query:expr, $q:ident => $value:expr) => {
        variant_field!($query, each [
            Q::PullRequest0, Q::PullRequestMatchString, Q::Issue2, Q::IssueMatchString,
            Q::Commit4, Q::CommitMatchString, Q::Compare6, Q::CompareMatchString,
        ] $q => $value)
    };
}

/// Binds `$q` to the query struct of an operation with changed files (all
/// but issues); `$other` answers for issues.
macro_rules! file_operation {
    ($query:expr, $q:ident => $value:expr, else => $other:expr) => {
        variant_field!($query, each [
            Q::PullRequest0, Q::PullRequestMatchString, Q::Commit4,
            Q::CommitMatchString, Q::Compare6, Q::CompareMatchString,
        ] $q => $value, else => $other)
    };
}

/// `Some($value)` with `$q` bound on an operation with `matchString`.
macro_rules! match_operation {
    ($query:expr, $q:ident => $value:expr) => {
        variant_field!($query, each [
            Q::PullRequestMatchString, Q::IssueMatchString, Q::CommitMatchString,
            Q::CompareMatchString,
        ] $q => Some($value), else => None)
    };
}

pub(crate) fn has_sections(query: &Q) -> bool {
    any_operation!(query, q => !q.sections.is_empty())
}
pub(super) fn operation(query: &Q) -> ItemOperation {
    match query {
        Q::PullRequest0(..) | Q::PullRequestMatchString(..) => ItemOperation::PullRequest,
        Q::Issue2(..) | Q::IssueMatchString(..) => ItemOperation::Issue,
        Q::Commit4(..) | Q::CommitMatchString(..) => ItemOperation::Commit,
        Q::Compare6(..) | Q::CompareMatchString(..) => ItemOperation::Compare,
    }
}
/// What a read names, taken from its variant's own fields: the item a
/// not-found answer says is missing.
pub(super) enum Subject<'a> {
    PullRequest(u64),
    Issue(u64),
    Commit(&'a str),
    Compare { base: &'a str, head: &'a str },
}
pub(super) fn subject(query: &Q) -> Subject<'_> {
    match query {
        Q::PullRequest0(q) => Subject::PullRequest(q.number.get()),
        Q::PullRequestMatchString(q) => Subject::PullRequest(q.number.get()),
        Q::Issue2(q) => Subject::Issue(q.number.get()),
        Q::IssueMatchString(q) => Subject::Issue(q.number.get()),
        Q::Commit4(q) => Subject::Commit(q.ref_.as_str()),
        Q::CommitMatchString(q) => Subject::Commit(q.ref_.as_str()),
        Q::Compare6(q) => Subject::Compare {
            base: q.base_ref.as_str(),
            head: q.head_ref.as_str(),
        },
        Q::CompareMatchString(q) => Subject::Compare {
            base: q.base_ref.as_str(),
            head: q.head_ref.as_str(),
        },
    }
}
pub(crate) fn owner(query: &Q) -> &str {
    any_operation!(query, q => q.owner.as_str())
}
pub(crate) fn repo(query: &Q) -> &str {
    any_operation!(query, q => q.repo.as_str())
}
pub(crate) fn number(query: &Q) -> Option<u64> {
    variant_field!(query, each [Q::PullRequest0, Q::PullRequestMatchString, Q::Issue2, Q::IssueMatchString]
        q => Some(q.number.get()), else => None)
}
/// Whether the read keeps the discussion entries a bot or app wrote
/// (`bots`, default true).
pub(crate) fn bots(query: &Q) -> bool {
    variant_field!(query, each [Q::PullRequest0, Q::PullRequestMatchString, Q::Issue2, Q::IssueMatchString]
        q => q.bots != Some(false), else => true)
}
pub(crate) fn reference(query: &Q) -> Option<&str> {
    variant_field!(query, each [Q::Commit4, Q::CommitMatchString]
        q => Some(q.ref_.as_str()), else => None)
}
pub(crate) fn base(query: &Q) -> Option<&str> {
    variant_field!(query, each [Q::Compare6, Q::CompareMatchString]
        q => Some(q.base_ref.as_str()), else => None)
}
pub(crate) fn head(query: &Q) -> Option<&str> {
    variant_field!(query, each [Q::Compare6, Q::CompareMatchString]
        q => Some(q.head_ref.as_str()), else => None)
}
pub(crate) fn include_patterns(query: &Q) -> Vec<&str> {
    file_operation!(query, q => q.include.iter().map(|x| x.as_str()).collect(), else => Vec::new())
}
pub(crate) fn head_sha(query: &Q) -> Option<&str> {
    variant_field!(query, each [Q::PullRequest0, Q::PullRequestMatchString]
        q => q.head_sha.as_ref().map(|x| x.as_str()), else => None)
}
pub(crate) fn page(query: &Q) -> Option<usize> {
    any_operation!(query, q => q.page.map(|x| saturating_usize(x.get())))
}
pub(crate) fn char_offset(query: &Q) -> Option<usize> {
    any_operation!(query, q => q.offset.map(saturating_usize))
}
pub(crate) fn has_file_filter(query: &Q) -> bool {
    file_operation!(
        query,
        q => !q.include.is_empty() || !q.file_status.is_empty(),
        else => false
    )
}
pub(crate) fn file_status(query: &Q) -> Vec<String> {
    file_operation!(query, q => q.file_status.iter().map(ToString::to_string).collect(), else => Vec::new())
}
pub(crate) fn match_strings(query: &Q) -> Option<&TextMatchTerms> {
    match_operation!(query, q => &q.match_string)
}
/// The lines kept around each `matchString` hit, in patch text and in a
/// matched body: the query's `contextLines`, else the contract's default
/// (`limits.defaultReadContextLines`).
pub(crate) fn match_context(query: &Q) -> (usize, usize) {
    const DEFAULT: (usize, usize) = (
        octocode_runtime_base::id::DEFAULT_READ_CONTEXT_LINES,
        octocode_runtime_base::id::DEFAULT_READ_CONTEXT_LINES,
    );
    variant_field!(query, each [
        Q::PullRequestMatchString, Q::IssueMatchString, Q::CommitMatchString,
        Q::CompareMatchString,
    ] q => q.context_lines.as_ref().map_or(DEFAULT, |c| {
        (
            octocode_runtime_base::num::usize_of_signed(c.before),
            octocode_runtime_base::num::usize_of_signed(c.after),
        )
    }), else => DEFAULT)
}
pub(crate) fn match_options(query: &Q) -> Option<(CaseMode, RegexEngine)> {
    match_operation!(query, q => (q.case_mode, q.regex_engine))
}
pub(crate) fn section_names(query: &Q) -> Vec<String> {
    any_operation!(query, q => q.sections.iter().map(ToString::to_string).collect())
}
pub(crate) fn collection_page_size(query: &Q) -> usize {
    page_size(query)
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(octocode_github::MAX_PER_PAGE)
}
pub(crate) fn later_page(query: &Q) -> bool {
    operation(query) == ItemOperation::PullRequest
        && (page(query).is_some_and(|p| p > 1) || char_offset(query).is_some())
}

pub(crate) fn page_size(query: &Q) -> Option<usize> {
    any_operation!(query, q => q.page_size.map(|x| saturating_usize(x.get())))
}
