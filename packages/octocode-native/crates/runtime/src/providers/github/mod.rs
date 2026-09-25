mod auth;
mod budget;
mod content;
mod dates;
mod endpoint;
mod error;
mod history;
mod history_item;
pub mod login;
mod query_syntax;
mod search;
mod transport;
mod tree;

pub use auth::{
    ChainedCredentialSource, ConfigCredentialResolver, CredentialRequest,
    CredentialResolutionHandle, CredentialResolver, CredentialSource, CredentialSourceProvider,
    GhCliCredentialSource, OAuthToken, OwnedCredentialRequest, PlatformCredentialStore,
    ResolvedCredential, StaticCredentialResolver, StoredCredentials, delete_platform_credential,
    load_stored_credentials, store_platform_credential, token_from_stored_blob,
};
pub use budget::{ExecutorConfig, GitHubBudget, GitHubResource, LimiterKey, session_snapshot};
pub use content::{
    CachePartition, CachedContent, ConditionalCache, ContentRequest, ContentResponse,
    GitHubProvider, NoCache,
};
pub use dates::resolve_date_window;
pub use endpoint::GitHubEndpoint;
pub use error::{ProviderError, ProviderErrorKind, ProviderErrorReason, RateLimit};
#[cfg(test)]
pub(crate) use history::MAX_PR_ONLY_PAGES_TO_SKIP;
pub use history::{
    CommitListRequest, HistoryPage, HistoryRequest, IssueListRequest, PullListRequest,
};
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
