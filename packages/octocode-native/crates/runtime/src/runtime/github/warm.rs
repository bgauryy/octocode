//! The batch warm-up: before a batch's rows run, what they share is asked
//! once instead of once per row.
//! - ghGetFileContent: each repository ref they name resolves once, and the
//!   files of one repository ref (two or more) are read together in one
//!   GraphQL request ([`octocode_github::GitHubProvider::warm_files_at_commits`]).
//! - ghGetHistoryItem: the first pages of the pull requests GraphQL serves
//!   (two or more) are read in one aliased request
//!   ([`gh_get_history_item::warm_pull_requests`]).
//!
//! Rows read the answers through their request context, which falls back to
//! its own request for anything the warm-up did not answer.
use super::{ExecutionContext, GitHubServices, ResponseAccess};
use futures_util::future::join_all;
use octocode_github::{RefMemo, SettleOnDrop};
use octocode_runtime_base::{id::ToolId, policy::path::PathPolicy, security::CheckedQuery};
use octocode_tool_github::{gh_get_file_content, gh_get_history_item};
use std::sync::Arc;

/// One batch's warm-up: what its rows share and the memo they read it from.
pub(crate) struct BatchWarm {
    pub(super) memo: Arc<RefMemo>,
    /// Refs to resolve, one per repository ref as the first row spelled it.
    refs: Vec<gh_get_file_content::WarmTarget>,
    /// Files read together: each group's repository ref and its paths.
    files: Vec<(gh_get_file_content::WarmTarget, Vec<String>)>,
    /// Pull-request rows whose first pages are read together.
    pull_requests: Vec<gh_get_history_item::PullRequestWarm>,
}

impl BatchWarm {
    /// The warm-up of `queries`; `None` when its rows share nothing to ask once.
    pub(crate) fn plan(tool: ToolId, queries: &[CheckedQuery], paths: &PathPolicy) -> Option<Self> {
        match tool {
            ToolId::GhGetFileContent => Self::file_reads(queries, paths),
            ToolId::GhGetHistoryItem => Self::pull_request_reads(queries),
            _ => None,
        }
    }

    fn file_reads(queries: &[CheckedQuery], paths: &PathPolicy) -> Option<Self> {
        let mut refs: Vec<gh_get_file_content::WarmTarget> = Vec::new();
        let mut files: Vec<(gh_get_file_content::WarmTarget, Vec<String>)> = Vec::new();
        for target in queries.iter().filter_map(|query| {
            gh_get_file_content::warm_target(query, |path| paths.withholds_repository_file(path))
        }) {
            match files.iter_mut().find(|(known, _)| known.same_ref(&target)) {
                Some((_, paths)) => {
                    if !paths.contains(&target.path) {
                        paths.push(target.path.clone());
                    }
                }
                None => files.push((target.clone(), vec![target.path.clone()])),
            }
            // One resolution per repository ref, as the first row spelled it.
            if target.resolves() && !refs.iter().any(|known| known.same_ref(&target)) {
                refs.push(target);
            }
        }
        // One file of a ref is its own read: nothing to share.
        files.retain(|(_, paths)| paths.len() > 1);
        if refs.is_empty() && files.is_empty() {
            return None;
        }
        let memo = if files.is_empty() {
            RefMemo::new()
        } else {
            RefMemo::new().warming_files()
        };
        Some(Self {
            memo: Arc::new(memo),
            refs,
            files,
            pull_requests: Vec::new(),
        })
    }

    fn pull_request_reads(queries: &[CheckedQuery]) -> Option<Self> {
        let mut pull_requests: Vec<gh_get_history_item::PullRequestWarm> = Vec::new();
        for warm in queries
            .iter()
            .filter_map(|query| gh_get_history_item::PullRequestWarm::of(query))
        {
            // A duplicate row sends the same read: its fill answers it.
            if !pull_requests.iter().any(|known| known.same_read(&warm)) {
                pull_requests.push(warm);
            }
        }
        (pull_requests.len() > 1).then(|| Self {
            memo: Arc::new(RefMemo::new().warming_pull_requests()),
            refs: Vec::new(),
            files: Vec::new(),
            pull_requests,
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
    /// Asks what the batch's rows share once. Anything it cannot answer is
    /// left to the rows.
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
            let resolved = join_all(warm.refs.iter().map(|target| async move {
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
            let commit_of = |target: &gh_get_file_content::WarmTarget| match &target.reference {
                Some(sha) if !target.resolves() => Some(sha.to_ascii_lowercase()),
                _ => resolved
                    .iter()
                    .find(|(known, _)| known.same_ref(target))
                    .and_then(|(_, sha)| sha.as_ref().ok().cloned()),
            };
            let commits = warm
                .files
                .iter()
                .filter_map(|(target, paths)| Some((target, paths, commit_of(target)?)))
                .collect::<Vec<_>>();
            let groups = commits
                .iter()
                .map(|(target, paths, commit)| octocode_github::FilesAtCommit {
                    owner: &target.owner,
                    repo: &target.repo,
                    commit,
                    paths,
                })
                .collect::<Vec<_>>();
            self.provider.warm_files_at_commits(&groups, request).await;
            if !warm.pull_requests.is_empty() {
                gh_get_history_item::warm_pull_requests(
                    &self.provider.transport,
                    &warm.pull_requests,
                    &warm.memo,
                    request,
                )
                .await;
            }
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
