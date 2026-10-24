//! The last commit touching a read file, cached per immutable (owner, repo, SHA, path).
use super::access;
use octocode_contract_types::GhGetFileContentQuery;
use octocode_github::{GitHubProvider, RequestContext};

/// Last commit touching `path` at `reference` (a resolved SHA), from the one
/// source of that fact ([`octocode_github::GitHubTransport::path_last_commit`],
/// which ghStructure's dated listings share). History below a commit is
/// immutable, so the answer is cached per (owner, repo, SHA, path) and
/// repeated offset-0 reads skip the round trip.
pub(super) async fn file_timestamp(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    reference: &str,
    context: &RequestContext,
) -> (Option<String>, Option<String>) {
    let partition = provider.transport.cache_partition(context, None).ok();
    let key = {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        for value in [
            access::owner(query).to_ascii_lowercase().as_str(),
            access::repo(query).to_ascii_lowercase().as_str(),
            reference,
            access::path(query),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        // v2: the author is the login when GitHub links one.
        format!(
            "github-file-timestamp:v2-{}",
            hex::encode(digest.finalize())
        )
    };
    if let Some(partition) = &partition
        && let Some(cached) = provider.transport.cache.get(partition, &key).await
        && let Ok((date, author)) =
            serde_json::from_slice::<(Option<String>, Option<String>)>(&cached.bytes)
    {
        return (date, author);
    }
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
    if let Some(partition) = &partition
        && let Ok(bytes) = serde_json::to_vec(&stamp)
    {
        provider
            .transport
            .cache
            .put(
                partition,
                key,
                octocode_github::CachedContent {
                    etag: None,
                    bytes: bytes.into(),
                    resolved_ref: reference.to_owned(),
                    has_more: false,
                },
                octocode_github::Retention::Pinned,
            )
            .await;
    }
    stamp
}
