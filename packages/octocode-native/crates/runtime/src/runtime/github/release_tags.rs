//! artifactSearch's release-tag and commit checks through a row's GitHub transport.
use super::{GitHubServices, ResponseAccess};
use octocode_github::ProviderErrorReason;
use octocode_runtime_base::execution::ExecutionContext;

/// Release-tag checks through a row's GitHub transport (see
/// [`GitHubServices::release_tags`]).
pub(crate) struct ReleaseTagCheck<'a> {
    pub(super) services: &'a GitHubServices,
    pub(super) context: &'a ExecutionContext,
    pub(super) access: &'a ResponseAccess,
}

impl octocode_tool_packages::artifact::ReleaseTags for ReleaseTagCheck<'_> {
    fn tag_commit<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tag: &'a str,
    ) -> octocode_tool_packages::artifact::TagCommitFuture<'a> {
        use octocode_tool_packages::artifact::CheckFailure;
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
                .map_err(|error| CheckFailure::provider(&error))
        })
    }

    fn tag_refs<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
    ) -> octocode_tool_packages::artifact::TagRefsFuture<'a> {
        use octocode_tool_packages::artifact::CheckFailure;
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .map_err(|_| CheckFailure::Request)?;
            self.services
                .provider
                .transport
                .repository_tag_refs(owner, repo, "", &request)
                .await
                .map_err(|error| CheckFailure::provider(&error))
        })
    }

    fn tag_commits<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tags: &'a [octocode_github::TagRef],
    ) -> octocode_tool_packages::artifact::TagCommitsFuture<'a> {
        use octocode_tool_packages::artifact::CheckFailure;
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .map_err(|_| CheckFailure::Request)?;
            self.services
                .provider
                .transport
                .tag_commits(owner, repo, tags, &request)
                .await
                .map_err(|error| CheckFailure::provider(&error))
        })
    }

    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> octocode_tool_packages::artifact::CommitFuture<'a> {
        use octocode_tool_packages::artifact::{CheckFailure, CommitCheck};
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
                    let to = octocode_tools_shared::gh_shared::canonical_repo(
                        &self.services.provider,
                        owner,
                        repo,
                        &request,
                        octocode_tools_shared::gh_shared::RenameCheck::Redirected,
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
                Err(error) => Err(CheckFailure::provider(&error)),
            }
        })
    }

    fn tree_files<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> octocode_tool_packages::artifact::TreeFilesFuture<'a> {
        use octocode_tool_packages::artifact::{CheckFailure, TreeFiles};
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .map_err(|_| CheckFailure::Request)?;
            let tree = self
                .services
                .provider
                .transport
                .get_tree(
                    &octocode_github::TreeRequest {
                        owner: owner.to_owned(),
                        repo: repo.to_owned(),
                        reference: commit.to_owned(),
                        recursive: true,
                    },
                    &request,
                )
                .await
                .map_err(|error| CheckFailure::provider(&error))?;
            Ok(TreeFiles {
                paths: tree
                    .tree
                    .into_iter()
                    .filter(|entry| entry.kind == "blob")
                    .map(|entry| entry.path)
                    .collect(),
                truncated: tree.truncated,
            })
        })
    }
}
