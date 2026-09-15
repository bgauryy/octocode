mod auth;
mod budget;
mod content;
mod dates;
mod endpoint;
mod error;
mod history;
mod history_item;
pub mod login;
mod search;
mod transport;
mod tree;

pub use auth::{
    ChainedCredentialSource, ConfigCredentialResolver, CredentialRequest,
    CredentialResolutionHandle, CredentialResolver, CredentialSource, CredentialSourceProvider,
    GhCliCredentialSource, LegacyCredentialStore, OAuthToken, OwnedCredentialRequest,
    PlatformCredentialStore, ResolvedCredential, StaticCredentialResolver, StoredCredentials,
    delete_platform_credential, load_stored_credentials, store_platform_credential,
    token_from_stored_blob,
};
pub use budget::{
    GitHubBudget, GitHubResource, graphql_is_skipped, session_snapshot, skip_graphql_host,
};
pub use content::{
    CachePartition, CachedContent, ConditionalCache, ContentRequest, ContentResponse,
    GitHubProvider, NoCache,
};
pub use dates::{quote_search_keyword, resolve_date_window};
pub use endpoint::GitHubEndpoint;
pub use error::{ProviderError, ProviderErrorKind, RateLimit};
pub use history::{
    CommitListRequest, HistoryPage, HistoryRequest, IssueListRequest, PullListRequest,
};
pub use history_item::HistoryItemResponse;
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
