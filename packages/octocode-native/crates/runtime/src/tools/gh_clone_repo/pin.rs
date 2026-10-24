//! The commit one clone checks out, resolved with the caller's credential: the access check a cached checkout is served behind.
use super::{CloneError, CloneFailure, GhCloneRepoQuery, repository_not_found};
use octocode_contract_types::ErrorCode;
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};

/// The commit one clone checks out, under the repository's canonical name,
/// and the ref name it was asked for (the default branch's name when none).
pub(super) struct Pin {
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) commit: String,
    pub(super) label: String,
    /// `owner`/`repo` is the canonical name of a renamed repository.
    pub(super) renamed: bool,
}

/// Resolve the clone's commit with the caller's credential. The clone cache
/// is shared by every credential of the runtime and its keys name none (a
/// refreshed token must not re-clone), so this read is also the access check
/// a cached checkout is served behind. Any failed answer refuses the row, a
/// rate limit or outage included: one caller's spent quota never unlocks
/// another caller's checkout. The reads follow GitHub's rename redirect, so
/// the canonical name costs no further request when nothing was renamed.
pub(super) async fn pin(
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    request: &RequestContext,
) -> Result<Pin, CloneFailure> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    let transport = &provider.transport;
    // Without a ref, the default branch's name and its head commit load
    // together: `HEAD` is that branch's head, so both answers name one commit.
    let (label, commit) = match query.ref_.as_deref() {
        Some(reference) => (
            reference.to_owned(),
            transport.commit_sha(owner, repo, reference, request).await,
        ),
        None => {
            let (metadata, head) = tokio::join!(
                transport.repository_metadata(owner, repo, request),
                transport.commit_sha(owner, repo, "HEAD", request),
            );
            match metadata {
                Ok(metadata) => (metadata.default_branch, head),
                Err(api) => return Err(refused(provider, query, "HEAD", api, request).await),
            }
        }
    };
    let commit = match commit {
        Ok(commit) => commit,
        Err(api) => return Err(refused(provider, query, &label, api, request).await),
    };
    let canonical = crate::tools::gh_shared::canonical_repo(
        provider,
        owner,
        repo,
        request,
        crate::tools::gh_shared::RenameCheck::Redirected,
    )
    .await
    .map_err(CloneFailure::Provider)?;
    let renamed = canonical.is_some();
    let (owner, repo) = canonical.unwrap_or_else(|| (owner.to_owned(), repo.to_owned()));
    Ok(Pin {
        owner,
        repo,
        commit,
        label,
        renamed,
    })
}

/// The failure of a clone whose commit lookup `api` refused ([`api_refusal`]);
/// a missing ref also names the repository's default branch and offers the
/// same clone at it.
async fn refused(
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    reference: &str,
    api: ProviderError,
    request: &RequestContext,
) -> CloneFailure {
    let missing_ref = api.reason == Some(ProviderErrorReason::RefNotFound);
    let mut failure = api_refusal(query, reference, api).unwrap_or_else(CloneFailure::Provider);
    if missing_ref
        && let CloneFailure::Clone(error) = &mut failure
        && let Some(retry) = crate::tools::gh_shared::default_branch_retry(
            provider,
            crate::id::ToolId::GhCloneRepo,
            crate::response::result::query_row(query, &[]),
            &["ref"],
            request,
        )
        .await
    {
        error.message = retry.message(&error.message);
        let (name, lead) = retry.lead();
        error
            .calls
            .get_or_insert_with(|| Box::new(serde_json::json!({})))[name] = lead;
    }
    failure
}

/// What a failed `commits/{reference}` answer says about the request: a
/// missing ref or repository is not-found input, a credential the API
/// rejects keeps its provider error. Any other failure (rate limit, outage,
/// timeout) says nothing about the request and is returned unclassified.
pub(super) fn api_refusal(
    query: &GhCloneRepoQuery,
    reference: &str,
    api: ProviderError,
) -> Result<CloneFailure, ProviderError> {
    match (api.kind, api.reason) {
        (_, Some(ProviderErrorReason::RefNotFound)) => {
            Ok(CloneFailure::Clone(CloneError {
                hints: vec![
                    "Verify the branch, tag, or SHA (ghStructure lists a ref), or omit ref for the default branch.".into(),
                ],
                calls: Some(Box::new(serde_json::json!({
                    "viewRefs": crate::tools::gh_shared::ref_recovery(&query.owner, &query.repo)
                }))),
                ..CloneError::new(
                    ErrorCode::NotFound,
                    format!(
                        "Branch, tag, or SHA not found for {}/{}: \"{reference}\"",
                        query.owner, query.repo
                    ),
                )
            }))
        }
        (ProviderErrorKind::NotFound, _) => Ok(CloneFailure::Clone(repository_not_found(query))),
        (ProviderErrorKind::Authentication | ProviderErrorKind::Permission, _) => {
            Ok(CloneFailure::Provider(api))
        }
        _ => Err(api),
    }
}
