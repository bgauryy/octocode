//! The batch warm-up of ghGetFileContent's refs: before a batch's rows run,
//! each repository ref they name resolves once instead of once per row.
//! Rows read the answers through their request context, which falls back to
//! its own request for any ref the warm-up did not answer.
use super::{ExecutionContext, GitHubServices, ResponseAccess};
use futures_util::future::join_all;
use octocode_github::{RefMemo, SettleOnDrop};
use octocode_runtime_base::{id::ToolId, policy::path::PathPolicy, security::CheckedQuery};
use octocode_tool_github::gh_get_file_content;
use std::sync::Arc;

/// One batch's warm-up: the refs its rows name and the memo they share.
pub(crate) struct BatchWarm {
    pub(super) memo: Arc<RefMemo>,
    targets: Vec<gh_get_file_content::WarmTarget>,
}

impl BatchWarm {
    /// The warm-up of `queries`' file reads by a ref to resolve; `None` when
    /// the tool reads no files or no row names such a ref.
    pub(crate) fn plan(tool: ToolId, queries: &[CheckedQuery], paths: &PathPolicy) -> Option<Self> {
        if tool != ToolId::GhGetFileContent {
            return None;
        }
        let mut targets: Vec<gh_get_file_content::WarmTarget> = Vec::new();
        for target in queries.iter().filter_map(|query| {
            gh_get_file_content::warm_target(query, |path| paths.withholds_repository_file(path))
        }) {
            // One resolution per repository ref, as the first row spelled it.
            if !targets.iter().any(|known| known.same_ref(&target)) {
                targets.push(target);
            }
        }
        (!targets.is_empty()).then(|| Self {
            memo: Arc::new(RefMemo::new()),
            targets,
        })
    }

    /// Settles the memo when dropped, so no row waits on a warm-up that ended.
    pub(crate) fn settling(&self) -> SettleOnDrop {
        SettleOnDrop(self.memo.clone())
    }

    /// The memo the rows read, for tests that watch what a warm-up leaves it.
    #[cfg(test)]
    pub(crate) fn memo(&self) -> &Arc<RefMemo> {
        &self.memo
    }
}

impl GitHubServices {
    /// Resolves each distinct ref once. Anything it cannot resolve is left
    /// to the rows.
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
            let resolved = join_all(warm.targets.iter().map(|target| async move {
                let sha = self
                    .provider
                    .resolve_reference(
                        &target.owner,
                        &target.repo,
                        target.reference.as_deref(),
                        request,
                    )
                    .await;
                (target, sha)
            }))
            .await;
            for (target, sha) in resolved {
                warm.memo.record_ref(
                    &target.owner,
                    &target.repo,
                    target.reference.as_deref(),
                    sha,
                );
            }
            warm.memo.observed_by(request);
            warm.memo.refs_known();
        });
        access.collect(&request);
    }
}
