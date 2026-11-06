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
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .ok()?;
            // A tag ref, not `commits/{tag}`: a branch of the same name is
            // no release tag.
            self.services
                .provider
                .transport
                .tag_commit(owner, repo, tag, &request)
                .await
                .ok()
        })
    }

    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> crate::providers::artifact::CommitFuture<'a> {
        use crate::providers::artifact::CommitCheck;
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .ok()?;
            match self
                .services
                .provider
                .transport
                .commit_sha(owner, repo, commit, &request)
                .await
            {
                Ok(_) if request.followed_rename(owner, repo) => {
                    Some(CommitCheck::FoundAfterRedirect)
                }
                Ok(_) => Some(CommitCheck::Found),
                Err(error) if error.reason == Some(ProviderErrorReason::RefNotFound) => {
                    Some(CommitCheck::Missing)
                }
                Err(error) if error.reason == Some(ProviderErrorReason::RepositoryNotFound) => {
                    Some(CommitCheck::RepositoryMissing)
                }
                Err(_) => None,
            }
        })
    }
}
