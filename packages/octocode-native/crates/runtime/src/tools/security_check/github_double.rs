//! A fake GitHub at the `RepositoryRead` boundary for tests: a repository read,
//! a commit lookup, and advisory pages answered by the cursor that asks them,
//! with every advisory request recorded.
use super::advisories::{AdvisoryFuture, AdvisoryRead};
use super::commit::{CommitFuture, CommitLookup};
use super::repository::{RepoFailure, RepoFuture, RepoRead, RepositoryRead};
use octocode_contract_types::ErrorCode;
use octocode_github::{AdvisoryPage, RepositoryAdvisory};
use std::sync::Mutex;

pub(crate) struct Double {
    repo: RepoRead,
    commit: CommitLookup,
    pages: Vec<(Option<String>, AdvisoryRead)>,
    asked: Mutex<Vec<Option<String>>>,
}

impl Double {
    pub(crate) fn repo(repo: RepoRead) -> Self {
        Self {
            repo,
            commit: CommitLookup::Missing,
            pages: Vec::new(),
            asked: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn commit(mut self, commit: CommitLookup) -> Self {
        self.commit = commit;
        self
    }

    /// The page asked by `after` (`None`: the first), naming `next` as the
    /// cursor of the page that follows.
    pub(crate) fn page(
        mut self,
        after: Option<&str>,
        advisories: Vec<RepositoryAdvisory>,
        next: Option<&str>,
    ) -> Self {
        self.pages.push((
            after.map(str::to_owned),
            AdvisoryRead::Page(AdvisoryPage {
                advisories,
                next: next.map(str::to_owned),
            }),
        ));
        self
    }

    pub(crate) fn page_read(mut self, after: Option<&str>, read: AdvisoryRead) -> Self {
        self.pages.push((after.map(str::to_owned), read));
        self
    }

    /// Every advisory request received, by its `after` cursor, in order.
    pub(crate) fn asked(&self) -> Vec<Option<String>> {
        self.asked.lock().expect("asked").clone()
    }
}

impl RepositoryRead for Double {
    fn read<'a>(&'a self, _: &'a str, _: &'a str) -> RepoFuture<'a> {
        let read = self.repo.clone();
        Box::pin(async move { read })
    }

    fn commit<'a>(&'a self, _: &'a str, _: &'a str, _: &'a str, _: &'a str) -> CommitFuture<'a> {
        let commit = self.commit.clone();
        Box::pin(async move { commit })
    }

    fn advisories<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        after: Option<&'a str>,
    ) -> AdvisoryFuture<'a> {
        self.asked
            .lock()
            .expect("asked")
            .push(after.map(str::to_owned));
        let page = self
            .pages
            .iter()
            .find(|(cursor, _)| cursor.as_deref() == after)
            .map(|(_, read)| read.clone())
            .unwrap_or_else(|| {
                AdvisoryRead::Failed(RepoFailure {
                    code: ErrorCode::NotFound,
                    why: format!("the double has no page after {after:?}"),
                })
            });
        Box::pin(async move { page })
    }
}

/// A row's network over `http` and `github`, kept answers none.
pub(super) fn net<'a>(
    http: &'a crate::providers::artifact::test_http::RouteHttp,
    budget: &'a octocode_io::budget::RequestBudget,
    reused: &'a crate::providers::saved_answers::ReusedSince,
    config: &'a crate::config::ResolvedConfig,
    github: &'a dyn RepositoryRead,
) -> super::Net<'a> {
    super::Net {
        http,
        budget,
        answers: None,
        reused,
        config,
        github: Some(github),
        osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
    }
}

/// The recorded advisory pages of lodash/lodash (GitHub's own answers).
pub(crate) fn recorded_advisories(page: usize) -> Vec<RepositoryAdvisory> {
    let text = match page {
        1 => include_str!("../../../tests/fixtures/github/security/advisories-1.json"),
        _ => include_str!("../../../tests/fixtures/github/security/advisories-2.json"),
    };
    serde_json::from_str(text).expect("recorded advisories")
}
