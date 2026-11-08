//! The last commit touching a read file.
use super::access;
use octocode_contract_types::GhGetFileContentQuery;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};

/// A file's `(date, author)` of its last change, or why the read failed.
pub(super) type Stamp = Result<(Option<String>, Option<String>), ProviderError>;

/// Last commit touching `path` at `reference` (a resolved SHA), from the one
/// source of that fact ([`octocode_github::GitHubTransport::path_last_commit`]).
pub(super) async fn file_timestamp(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    reference: &str,
    context: &RequestContext,
) -> Stamp {
    provider
        .transport
        .path_last_commit(
            access::owner(query),
            access::repo(query),
            reference,
            access::path(query),
            context,
        )
        .await
}

/// The row's note that `lastModified` is absent because its read failed: the
/// body stands, and the reason is the error's code, never silence.
pub(super) fn unavailable_note(error: &ProviderError) -> String {
    format!(
        "lastModified is absent: the last-change read failed ({})",
        error.kind.code()
    )
}
