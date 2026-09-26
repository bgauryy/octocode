//! GitHub host integration: credential policy, OAuth, and relative query dates.
mod auth;
mod dates;
pub mod login;

pub use auth::{
    AuthMode, AuthSelection, Authentication, CredentialRequest, CredentialResolver,
    CredentialSource, CredentialStore, OAuthToken, ResolvedCredential, StaticCredentialResolver,
    StoredCredentials, delete_platform_credential, load_stored_credentials,
    store_platform_credential, token_from_stored_blob,
};
pub use dates::resolve_date_window;
pub use octocode_github::{
    CachePartition, CachedContent, CodeSearchItem, CodeSearchPage, CodeSearchRequest,
    CommitListRequest, ConditionalCache, ContentRequest, ContentResponse, ContentsEntry,
    ContentsListing, ExecutorConfig, GitHubBudget, GitHubEndpoint, GitHubProvider, GitHubResource,
    GitHubTransport, GraphQlError, GraphQlPage, HistoryPage, HistoryRequest, HttpMethod,
    IssueListRequest, LimiterKey, NoCache, ProviderError, ProviderErrorKind, ProviderErrorReason,
    PullListRequest, RateLimit, RepositoryMetadata, RepositorySearchPage, RepositorySearchRequest,
    RequestContext, RequestSpec, ResponsePage, RetryPolicy, SearchName, TextMatch, TreeEntry,
    TreeRequest, TreeResponse, qualifier_value, quote_search_keyword, search_phrase,
    session_snapshot, validate_qualifier_value, validate_search_name,
};
