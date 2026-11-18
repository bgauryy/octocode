//! A fake GitHub at the `RepositoryRead` boundary for tests: a repository read,
//! a commit lookup, and advisory pages answered by the cursor that asks them,
//! with every advisory request recorded.
use super::advisories::{AdvisoryFuture, AdvisoryRead, GhsaFuture, GhsaRead};
use super::commit::{CommitFuture, CommitLookup};
use super::repository::{RepoFailure, RepoFuture, RepoRead, RepositoryRead};
use octocode_contract_types::ErrorCode;
use octocode_github::{AdvisoryPage, RepositoryAdvisory};
use std::sync::Mutex;

pub(crate) struct Double {
    repo: RepoRead,
    /// Reads by `owner/name`; a repository not given here reads as `repo`.
    repos: Vec<(String, RepoRead)>,
    repos_asked: Mutex<Vec<String>>,
    commit: CommitLookup,
    pages: Vec<(Option<String>, AdvisoryRead)>,
    asked: Mutex<Vec<Option<String>>>,
    /// Advisories by GHSA id: `(repo, id)` with `repo` `None` for the global database.
    ghsas: Vec<(Option<String>, String, GhsaRead)>,
    /// GitHub's global database pages by `after`; none given: one empty page.
    global: Vec<(Option<String>, AdvisoryRead)>,
    global_asked: Mutex<Vec<(String, String, Option<String>)>>,
    ghsa_asked: Mutex<Vec<(Option<String>, String)>>,
    /// Every read answers as one made without a token.
    anonymous: bool,
}

impl Double {
    pub(crate) fn repo(repo: RepoRead) -> Self {
        Self {
            repo,
            repos: Vec::new(),
            repos_asked: Mutex::new(Vec::new()),
            commit: CommitLookup::Missing,
            pages: Vec::new(),
            asked: Mutex::new(Vec::new()),
            ghsas: Vec::new(),
            ghsa_asked: Mutex::new(Vec::new()),
            global: Vec::new(),
            global_asked: Mutex::new(Vec::new()),
            anonymous: false,
        }
    }

    /// Reads answer as ones made without a token.
    pub(crate) fn read_anonymously(mut self) -> Self {
        self.anonymous = true;
        self
    }

    /// What GitHub answers for `repo` (`owner/name`) alone.
    pub(crate) fn repo_at(mut self, repo: &str, read: RepoRead) -> Self {
        self.repos.push((repo.to_owned(), read));
        self
    }

    /// Every repository read received, as `owner/name`, in order.
    pub(crate) fn repos_asked(&self) -> Vec<String> {
        self.repos_asked.lock().expect("asked").clone()
    }

    /// The global database's page asked by `after` (`None`: the first).
    pub(crate) fn global_page(mut self, after: Option<&str>, read: AdvisoryRead) -> Self {
        self.global.push((after.map(str::to_owned), read));
        self
    }

    /// Every global database request, as `(ecosystem, affects, after)`.
    pub(crate) fn global_asked(&self) -> Vec<(String, String, Option<String>)> {
        self.global_asked.lock().expect("asked").clone()
    }

    /// The advisory `id` as `repo` (`owner/name`) keeps it, or GitHub's global
    /// database (`None`); an id not given here is missing.
    pub(crate) fn ghsa(mut self, repo: Option<&str>, id: &str, read: GhsaRead) -> Self {
        self.ghsas
            .push((repo.map(str::to_owned), id.to_owned(), read));
        self
    }

    /// Every GHSA read received, as `(repo, id)`, in order.
    pub(crate) fn ghsa_asked(&self) -> Vec<(Option<String>, String)> {
        self.ghsa_asked.lock().expect("asked").clone()
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
    fn read<'a>(&'a self, owner: &'a str, name: &'a str) -> RepoFuture<'a> {
        let repo = format!("{owner}/{name}");
        let read = self
            .repos
            .iter()
            .find(|(at, _)| *at == repo)
            .map_or_else(|| self.repo.clone(), |(_, read)| read.clone());
        self.repos_asked.lock().expect("asked").push(repo);
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

    fn global_advisories<'a>(
        &'a self,
        ecosystem: &'a str,
        affects: &'a str,
        after: Option<&'a str>,
    ) -> AdvisoryFuture<'a> {
        self.global_asked.lock().expect("asked").push((
            ecosystem.to_owned(),
            affects.to_owned(),
            after.map(str::to_owned),
        ));
        let read = if self.global.is_empty() && after.is_none() {
            AdvisoryRead::Page(AdvisoryPage {
                advisories: Vec::new(),
                next: None,
            })
        } else {
            self.global
                .iter()
                .find(|(cursor, _)| cursor.as_deref() == after)
                .map(|(_, read)| read.clone())
                .unwrap_or_else(|| {
                    AdvisoryRead::Failed(RepoFailure {
                        code: ErrorCode::NotFound,
                        why: format!("the double has no global page after {after:?}"),
                    })
                })
        };
        Box::pin(async move { read })
    }

    fn ghsa<'a>(&'a self, repo: Option<(&'a str, &'a str)>, id: &'a str) -> GhsaFuture<'a> {
        let repo = repo.map(|(owner, name)| format!("{owner}/{name}"));
        self.ghsa_asked
            .lock()
            .expect("asked")
            .push((repo.clone(), id.to_owned()));
        let read = self
            .ghsas
            .iter()
            .find(|(at, known, _)| *at == repo && known == id)
            .map_or(GhsaRead::Missing, |(_, _, read)| read.clone());
        Box::pin(async move { read })
    }

    fn anonymous(&self) -> bool {
        self.anonymous
    }
}

/// A row's network over `http` and `github`, kept answers none.
pub(super) fn net<'a>(
    http: &'a crate::artifact::test_http::RouteHttp,
    budget: &'a octocode_io::budget::RequestBudget,
    reused: &'a octocode_runtime_base::providers::saved_answers::ReusedSince,
    config: &'a octocode_runtime_base::config::ResolvedConfig,
    github: &'a dyn RepositoryRead,
) -> super::Net<'a> {
    super::Net {
        http,
        budget,
        answers: None,
        reused,
        config,
        github: Some(github),
        osv_api: octocode_runtime_base::execution::RuntimeLimits::DEFAULT.osv_api,
    }
}

/// The recorded advisory pages of lodash/lodash (GitHub's own answers).
pub(crate) fn recorded_advisories(page: usize) -> Vec<RepositoryAdvisory> {
    let text = match page {
        1 => include_str!("../../../runtime/tests/fixtures/github/security/advisories-1.json"),
        _ => include_str!("../../../runtime/tests/fixtures/github/security/advisories-2.json"),
    };
    serde_json::from_str(text).expect("recorded advisories")
}
