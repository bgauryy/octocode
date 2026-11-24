//! GitHub protocol, bounded transport, and provider operations.
//! Hosts own credential acquisition and application policy.
// Numeric conversions are explicit (`try_from` with a stated outcome): a
// lossy `as` cast fails CI (`-D warnings`). Tests may cast fixture values.
#![cfg_attr(
    not(test),
    warn(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )
)]
// Library output reaches people through its callers or the `octocode_io::notice`
// owner, never stray stdout/stderr prints; deliberate sites carry `#[expect]`.
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod budget;
mod content;
mod credentials;
mod endpoint;
mod error;
mod history;
mod history_item;
mod partition;
mod query_syntax;
mod ref_memo;
mod refs;
mod search;
mod security;
mod transport;
mod tree;

/// GitHub REST's largest `per_page`: one provider batch of a list endpoint
/// (branches, tags, comments, reviews, commits, changed files).
pub const MAX_PER_PAGE: usize = 100;

pub use budget::{AuthAdmission, GitHubBudget, GitHubPacing, GitLimiter, LimiterKey};
pub use content::{
    ContentRequest, ContentResponse, FilesAtCommit, GitHubProvider, Submodule, decode_content,
    is_full_sha,
};
pub use credentials::{CredentialSource, ResolvedCredential};
pub use endpoint::{
    Deployment, GitHubEndpoint, api_base_for_host, credential_host, is_loopback, names_localhost,
    proxy_for,
};
pub use error::{ProviderError, ProviderErrorKind, ProviderErrorReason, RateLimit};
pub use history::{CommitListRequest, HistoryPage, HistoryRequest, PullListRequest};
pub use partition::Partition;
pub use query_syntax::{
    qualifier_value, quote_search_keyword, search_phrase, validate_person_name,
};
pub use ref_memo::{RefMemo, SettleOnDrop};
pub use refs::{NamedRef, RefKind, RefMatch, RefPage, TagObject, TagRef};
pub use search::{
    CodeSearchItem, CodeSearchPage, CodeSearchRequest, RepositoryMetadata, RepositorySearchItem,
    RepositorySearchPage, RepositorySearchRequest, SEARCH_RESULT_CAP, SearchRepository, TextMatch,
    TreeEntry, TreeRequest, TreeResponse,
};
pub use security::{
    AdvisoryCredit, AdvisoryCvss, AdvisoryCwe, AdvisoryEpss, AdvisoryPackage, AdvisoryPage,
    AdvisoryUser, AdvisoryVulnerability, CommitRead, CvssSeverities, RefHolder, RefHolders,
    RepositoryAdvisory, tags_pointing_at,
};
pub use transport::{
    AnswerBody, GitHubTransport, GraphQlError, GraphQlPage, HTTP_USER_AGENT, HttpMethod,
    ProviderAnswer, ProviderEvidence, RequestContext, RequestSpec, ResponsePage, RetryPolicy,
    Reuse, SEARCH_REUSE_WINDOW, SavedAnswer, SavedAnswers, requested_file, sso_partial_warning,
    worth_keeping,
};
pub use tree::{ContentsEntry, ContentsListing};

#[cfg(test)]
mod tests;
