//! Release-tag checks one call's rows share: each check is made once per call
//! ([`crate::artifact::CallShare`]), so rows naming one repository (two versions of a
//! package, the same package twice) ask GitHub for each tag, list, commit and
//! tree once.
use super::{
    CheckFailure, CommitFuture, CommitResult, ReleaseTags, TagCommitFuture, TagCommitsFuture,
    TagRefsFuture, TreeFiles, TreeFilesFuture,
};
use crate::artifact::share::Flights;
use octocode_github::TagRef;

/// The checks a call's rows made, by repository and what was asked.
#[derive(Default)]
pub(crate) struct TagShare {
    tags: Flights<Result<Option<String>, CheckFailure>>,
    lists: Flights<Result<Vec<TagRef>, CheckFailure>>,
    resolved: Flights<Result<Vec<Option<String>>, CheckFailure>>,
    commits: Flights<CommitResult>,
    trees: Flights<Result<TreeFiles, CheckFailure>>,
}

/// `inner`'s checks, each made once in the call `share` belongs to.
pub(crate) struct SharedTags<'s> {
    pub inner: &'s dyn ReleaseTags,
    pub share: &'s TagShare,
}

impl ReleaseTags for SharedTags<'_> {
    fn tag_commit<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tag: &'a str,
    ) -> TagCommitFuture<'a> {
        Box::pin(self.share.tags.once(
            format!("{owner}/{repo}@{tag}"),
            self.inner.tag_commit(owner, repo, tag),
        ))
    }

    fn tag_refs<'a>(&'a self, owner: &'a str, repo: &'a str) -> TagRefsFuture<'a> {
        Box::pin(
            self.share
                .lists
                .once(format!("{owner}/{repo}"), self.inner.tag_refs(owner, repo)),
        )
    }

    fn tag_commits<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tags: &'a [TagRef],
    ) -> TagCommitsFuture<'a> {
        let names = tags
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Box::pin(self.share.resolved.once(
            format!("{owner}/{repo}\n{names}"),
            self.inner.tag_commits(owner, repo, tags),
        ))
    }

    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> CommitFuture<'a> {
        Box::pin(self.share.commits.once(
            format!("{owner}/{repo}@{commit}"),
            self.inner.commit_check(owner, repo, commit),
        ))
    }

    fn tree_files<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> TreeFilesFuture<'a> {
        Box::pin(self.share.trees.once(
            format!("{owner}/{repo}@{commit}"),
            self.inner.tree_files(owner, repo, commit),
        ))
    }
}
