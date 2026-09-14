mod auth;
mod budget;
mod content;
mod dates;
mod endpoint;
mod error;
mod history;
mod history_item;
mod search;
mod transport;
mod tree;

pub use auth::{
    ChainedCredentialSource, ConfigCredentialResolver, CredentialRequest,
    CredentialResolutionHandle, CredentialResolver, CredentialSource, CredentialSourceProvider,
    GhCliCredentialSource, LegacyCredentialStore, OAuthToken, OwnedCredentialRequest,
    PlatformCredentialStore, ResolvedCredential, StaticCredentialResolver, StoredCredentials,
    delete_platform_credential, load_stored_credential, store_platform_credential,
};
pub use budget::{GitHubBudget, GitHubResource, session_snapshot};
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
