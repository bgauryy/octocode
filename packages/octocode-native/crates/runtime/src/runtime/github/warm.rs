//! The batch warm-up of ghGetFileContent's last commits: before a batch's
//! rows run, the commits their refs name and the last commit of each file
//! come from one aliased GraphQL request per repository commit instead of
//! one request per file. Rows read the answers through their request
//! context, which falls back to its own request for any path the warm-up
//! did not answer.
use super::{ExecutionContext, GitHubServices, ResponseAccess};
use crate::{
    id::ToolId, policy::path::PathPolicy, security::CheckedQuery, tools::gh_get_file_content,
};
use futures_util::future::join_all;
use octocode_github::{LastCommitMemo, LastCommitWant, SettleOnDrop, is_full_sha};
use std::sync::Arc;

/// One batch's warm-up: the files its rows read and the memo they share.
pub(crate) struct BatchWarm {
    pub(super) memo: Arc<LastCommitMemo>,
    targets: Vec<gh_get_file_content::WarmTarget>,
}

impl BatchWarm {
    /// The warm-up of `queries`' first-page file reads; `None` when the tool
    /// reads no files or no row is such a read.
    pub(crate) fn plan(tool: ToolId, queries: &[CheckedQuery], paths: &PathPolicy) -> Option<Self> {
        if tool != ToolId::GhGetFileContent {
            return None;
        }
        let targets = queries
            .iter()
            .filter_map(|query| {
                gh_get_file_content::warm_target(query, |path| {
                    paths.withholds_repository_file(path)
                })
            })
            .collect::<Vec<_>>();
        (!targets.is_empty()).then(|| Self {
            memo: Arc::new(LastCommitMemo::new()),
            targets,
        })
    }

    /// Settles the memo when dropped, so no row waits on a warm-up that ended.
    pub(crate) fn settling(&self) -> SettleOnDrop {
        SettleOnDrop(self.memo.clone())
    }

    /// The memo the rows read, for tests that watch what a warm-up leaves it.
    #[cfg(test)]
    pub(crate) fn memo(&self) -> &Arc<LastCommitMemo> {
        &self.memo
    }
}

impl GitHubServices {
    /// Resolves each distinct ref once, then asks every file's last commit.
    /// Anything it cannot resolve or answer is left to the rows.
    pub(crate) fn warm(
        &self,
        warm: &BatchWarm,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
        access: &ResponseAccess,
    ) {
        // No memo on this context: the warm-up must not wait on itself.
        let Ok(request) = self.request_context(context, handle, access) else {
            return;
        };
        handle.block_on(async {
            let request = &request;
            // One resolution per repository ref, as the first row spelled it.
            let mut refs: Vec<&gh_get_file_content::WarmTarget> = Vec::new();
            for target in &warm.targets {
                let same = |known: &gh_get_file_content::WarmTarget| {
                    known.owner.eq_ignore_ascii_case(&target.owner)
                        && known.repo.eq_ignore_ascii_case(&target.repo)
                        && known.reference == target.reference
                };
                if !refs.iter().any(|known| same(known)) {
                    refs.push(target);
                }
            }
            let resolved = join_all(refs.iter().map(|target| async move {
                let sha = self
                    .provider
                    .resolve_reference(
                        &target.owner,
                        &target.repo,
                        target.reference.as_deref(),
                        request,
                    )
                    .await;
                (*target, sha)
            }))
            .await;
            for (target, sha) in &resolved {
                if !target.reference.as_deref().is_some_and(is_full_sha) {
                    warm.memo.record_ref(
                        &target.owner,
                        &target.repo,
                        target.reference.as_deref(),
                        sha.clone(),
                    );
                }
            }
            warm.memo.observed_by(request);
            warm.memo.refs_known();
            let wants = warm
                .targets
                .iter()
                .filter_map(|target| {
                    let (_, sha) = resolved.iter().find(|(known, _)| {
                        known.owner.eq_ignore_ascii_case(&target.owner)
                            && known.repo.eq_ignore_ascii_case(&target.repo)
                            && known.reference == target.reference
                    })?;
                    Some(LastCommitWant {
                        owner: target.owner.clone(),
                        repo: target.repo.clone(),
                        sha: sha.as_ref().ok()?.clone(),
                        path: target.path.clone(),
                    })
                })
                .collect::<Vec<_>>();
            self.provider
                .transport
                .warm_last_commits(&wants, &warm.memo, request)
                .await;
        });
        access.collect(&request);
    }
}
