//! GitHub protocol, bounded transport, and provider operations.
//! Hosts own credential acquisition, cache storage, and application policy.

mod budget;
mod cache_key;
mod commit_dates;
mod content;
mod credentials;
mod endpoint;
mod error;
mod history;
mod history_item;
mod partition;
mod query_syntax;
mod refs;
mod search;
mod transport;
mod tree;

use octocode_io::flight::KeyedFlight;

/// GitHub REST's largest `per_page`: one provider batch of a list endpoint
/// (branches, tags, comments, reviews, commits, changed files).
pub const MAX_PER_PAGE: usize = 100;

pub use budget::{AuthAdmission, GitHubBudget, GitHubPacing, LimiterKey};
pub use cache_key::fields_key;
pub use commit_dates::{MAX_PATHS_PER_REQUEST, PathDates};
pub use content::{
    CachedContent, ConditionalCache, ContentRequest, ContentResponse, GitHubProvider, NoCache,
    Retention, Submodule, decode_content, is_full_sha,
};
pub use credentials::{CredentialSource, ResolvedCredential};
pub use endpoint::{GitHubEndpoint, credential_host, is_loopback, names_localhost};
pub use error::{ProviderError, ProviderErrorKind, ProviderErrorReason, RateLimit};
pub use history::{CommitListRequest, HistoryPage, HistoryRequest, PullListRequest};
pub use partition::Partition;
pub use query_syntax::{
    qualifier_value, quote_search_keyword, search_phrase, validate_person_name,
    validate_qualifier_value,
};
pub use refs::{NamedRef, RefKind, RefMatch, RefPage};
pub use search::{
    CodeSearchItem, CodeSearchPage, CodeSearchRequest, RepositoryMetadata, RepositorySearchItem,
    RepositorySearchPage, RepositorySearchRequest, TextMatch, TreeEntry, TreeRequest, TreeResponse,
};
pub use transport::{
    GitHubTransport, GraphQlError, GraphQlPage, HTTP_USER_AGENT, HttpMethod, RequestContext,
    RequestSpec, ResponsePage, RetryPolicy, sso_partial_warning,
};
pub use tree::{ContentsEntry, ContentsListing};

#[cfg(any(test, feature = "test-support"))]
pub mod mock_http;
#[cfg(test)]
mod tests;
