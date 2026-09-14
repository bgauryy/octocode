mod auth;
mod budget;
mod content;
mod dates;
mod device;
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
    GhCliCredentialSource, LegacyCredentialStore, OAuthToken, OCTOCODE_GITHUB_APP_CLIENT_ID,
    OwnedCredentialRequest, PlatformCredentialStore, ResolvedCredential, StaticCredentialResolver,
    StoredCredentials, delete_credentials_value, delete_platform_credential, get_credentials_value,
    get_token_with_refresh, get_token_with_refresh_value, load_stored_credential,
    refresh_auth_token, refresh_auth_token_value, store_credentials_value,
    store_platform_credential,
};
pub use budget::{GitHubBudget, GitHubResource, session_snapshot};
pub use content::{
    CachePartition, CachedContent, ConditionalCache, ContentRequest, ContentResponse,
    GitHubProvider, NoCache,
};
pub use dates::{quote_search_keyword, resolve_date_window};
pub use device::{
    AUTH_LOGIN_HINT, DeviceClient, GITHUB_APP_CLIENT_ID, LoginOrigins, PlatformIo, open_url,
    parse_scopes, refresh_auth_token, refresh_storage_if_needed,
};
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
