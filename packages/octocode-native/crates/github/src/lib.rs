//! GitHub protocol, bounded transport, and provider operations.
//! Hosts own credential acquisition, cache storage, and application policy.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

mod budget;
mod content;
mod credentials;
mod endpoint;
mod error;
mod history;
mod history_item;
mod query_syntax;
mod search;
mod transport;
mod tree;

pub use budget::{
    AuthAdmission, ExecutorConfig, GitHubBudget, GitHubResource, LimiterKey, session_snapshot,
};
pub use content::{
    CachePartition, CachedContent, ConditionalCache, ContentRequest, ContentResponse,
    GitHubProvider, NoCache,
};
pub use credentials::{
    CredentialRequest, CredentialResolver, CredentialSource, ResolvedCredential,
    StaticCredentialResolver,
};
pub use endpoint::GitHubEndpoint;
pub use error::{ProviderError, ProviderErrorKind, ProviderErrorReason, RateLimit};
pub use history::{CommitListRequest, HistoryPage, HistoryRequest, PullListRequest};
pub use query_syntax::{
    SearchName, qualifier_value, quote_search_keyword, search_phrase, validate_qualifier_value,
    validate_search_name,
};
pub use search::{
    CodeSearchItem, CodeSearchPage, CodeSearchRequest, RepositoryMetadata, RepositorySearchPage,
    RepositorySearchRequest, TextMatch, TreeEntry, TreeRequest, TreeResponse,
};
pub use transport::{
    GitHubTransport, GraphQlError, GraphQlPage, HttpMethod, RequestContext, RequestSpec,
    ResponsePage, RetryPolicy,
};
pub use tree::{ContentsEntry, ContentsListing};

#[cfg(test)]
mod tests;
