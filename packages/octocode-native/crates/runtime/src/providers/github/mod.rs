//! GitHub host integration: credential policy, OAuth, and relative query dates.
mod auth;
mod dates;
pub mod login;

pub use auth::{
    AuthMode, AuthSelection, Authentication, CredentialSource, CredentialStore, OAuthToken,
    ResolvedCredential, StoredCredentials, configured_credential_host,
};
pub use dates::{resolve_date_window, utc_timestamp};
#[cfg(test)]
pub(crate) use octocode_github::NoCache;
pub use octocode_github::{
    CachePartition, CachedContent, CodeSearchItem, CodeSearchRequest, CommitListRequest,
    ConditionalCache, ContentRequest, ContentResponse, ContentsEntry, GitHubBudget, GitHubEndpoint,
    GitHubProvider, GitHubTransport, GraphQlError, HistoryPage, HistoryRequest, LimiterKey,
    NamedRef, ProviderError, ProviderErrorKind, ProviderErrorReason, PullListRequest, RateLimit,
    RefKind, RepositorySearchItem, RepositorySearchPage, RepositorySearchRequest, RequestContext,
    RequestSpec, RetryPolicy, SearchName, TextMatch, TreeRequest, TreeResponse, qualifier_value,
    quote_search_keyword, search_phrase, validate_qualifier_value, validate_search_name,
};
