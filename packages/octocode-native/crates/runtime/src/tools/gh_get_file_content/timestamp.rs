//! The last commit touching a read file.
use super::access;
use octocode_contract_types::GhGetFileContentQuery;
use octocode_github::{GitHubProvider, RequestContext};

/// Last commit touching `path` at `reference` (a resolved SHA), from the one
/// source of that fact ([`octocode_github::GitHubTransport::path_last_commit`]).
pub(super) async fn file_timestamp(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    reference: &str,
    context: &RequestContext,
) -> (Option<String>, Option<String>) {
    let Ok(stamp) = provider
        .transport
        .path_last_commit(
            access::owner(query),
            access::repo(query),
            reference,
            access::path(query),
            context,
        )
        .await
    else {
        return (None, None);
    };
    stamp
}
