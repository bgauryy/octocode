//! artifactSearch's release-tag and commit checks through a row's GitHub transport.
use super::{GitHubServices, ResponseAccess};
use crate::execution::ExecutionContext;
use octocode_github::ProviderErrorReason;

/// Release-tag checks through a row's GitHub transport (see
/// [`GitHubServices::release_tags`]).
pub(crate) struct ReleaseTagCheck<'a> {
    pub(super) services: &'a GitHubServices,
    pub(super) context: &'a ExecutionContext,
    pub(super) access: &'a ResponseAccess,
}

impl crate::providers::artifact::ReleaseTags for ReleaseTagCheck<'_> {
    fn tag_commit<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tag: &'a str,
    ) -> crate::providers::artifact::TagCommitFuture<'a> {
        use crate::providers::artifact::CheckFailure;
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .map_err(|_| CheckFailure::Request)?;
            // A tag ref, not `commits/{tag}`: a branch of the same name is
            // no release tag.
            self.services
                .provider
                .transport
                .tag_commit(owner, repo, tag, &request)
                .await
                .map_err(|error| CheckFailure::Provider(error.kind))
        })
    }

    fn tag_names<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
    ) -> crate::providers::artifact::TagNamesFuture<'a> {
        use crate::providers::artifact::CheckFailure;
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .map_err(|_| CheckFailure::Request)?;
            self.services
                .provider
                .transport
                .repository_tag_names(owner, repo, &request)
                .await
                .map_err(|error| CheckFailure::Provider(error.kind))
        })
    }

    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> crate::providers::artifact::CommitFuture<'a> {
        use crate::providers::artifact::{CheckFailure, CommitCheck};
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .map_err(|_| CheckFailure::Request)?;
            match self
                .services
                .provider
                .transport
                .commit_sha(owner, repo, commit, &request)
                .await
            {
                Ok(_) if request.followed_rename(owner, repo) => {
                    // The redirect names the repository's id, not its new
                    // name: the repository's metadata names it. A read that
                    // fails leaves the target unnamed, not the check failed.
                    let to = crate::tools::gh_shared::canonical_repo(
                        &self.services.provider,
                        owner,
                        repo,
                        &request,
                        crate::tools::gh_shared::RenameCheck::Redirected,
                    )
                    .await
                    .ok()
                    .flatten()
                    .map(|(owner, repo)| format!("{owner}/{repo}"));
                    Ok(CommitCheck::FoundAfterRedirect { to })
                }
                Ok(_) => Ok(CommitCheck::Found),
                Err(error) if error.reason == Some(ProviderErrorReason::RefNotFound) => {
                    Ok(CommitCheck::Missing)
                }
                Err(error) if error.reason == Some(ProviderErrorReason::RepositoryNotFound) => {
                    Ok(CommitCheck::RepositoryMissing)
                }
                Err(error) => Err(CheckFailure::Provider(error.kind)),
            }
        })
    }
}
