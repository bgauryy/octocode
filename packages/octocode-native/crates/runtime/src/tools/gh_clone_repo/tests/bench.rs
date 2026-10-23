//! Mock-API bench: the fixture repository behind a wiremock GitHub API, and clones run as one caller.
use super::*;

/// A fixture context whose caller did not resolve a default branch, as the
/// runtime now calls it (no GitHub API before cloning).
pub(super) struct Bench {
    pub(super) fixture: Arc<Fixture>,
    /// Holds the temp directory for the bench's lifetime.
    pub(super) _root: test_support::Fixture,
    pub(super) policy: PathPolicy,
    pub(super) endpoint: GitHubEndpoint,
    pub(super) runner: RewriteRunner,
    pub(super) config: CloneConfig,
}

impl Bench {
    pub(super) fn new() -> Self {
        Self::at(api_github_endpoint())
    }

    /// A bench whose GitHub API is `endpoint`; git clones the shared
    /// fixture in place of the repository URL that endpoint derives.
    pub(super) fn at(endpoint: GitHubEndpoint) -> Self {
        Self::over(endpoint, Fixture::shared())
    }

    /// [`Self::at`] over its own fixture, for a test that commits to it.
    pub(super) fn committing(endpoint: GitHubEndpoint) -> Self {
        Self::over(endpoint, Arc::new(Fixture::new()))
    }

    pub(super) fn over(endpoint: GitHubEndpoint, fixture: Arc<Fixture>) -> Self {
        let (root, cache_home) = home_root();
        let policy = test_support::workspace_policy(&root.0);
        let url = repository_url(
            &web_root(&endpoint).expect("root"),
            "fixture-owner",
            "fixture-repo",
        )
        .expect("clone URL");
        let runner = RewriteRunner::new(&url, &fixture.bare_url);
        let mut config = CloneConfig::persistent(&cache_home);
        config.cache_ttl = Duration::from_secs(60);
        Self {
            fixture,
            _root: root,
            policy,
            endpoint,
            runner,
            config,
        }
    }

    pub(super) fn context(&self) -> CloneContext<'_> {
        self.context_as(None)
    }

    /// [`Self::context`] for the caller holding `credential`.
    pub(super) fn context_as<'a>(
        &'a self,
        credential: Option<&'a ResolvedCredential>,
    ) -> CloneContext<'a> {
        CloneContext {
            ..setup(
                &self.runner,
                &self.config,
                &self.endpoint,
                &self.policy,
                &NeverCancel,
                credential,
            )
        }
    }

    pub(super) fn clones_seen(&self) -> usize {
        self.runner
            .seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|call| call.contains("\"clone\"") || call.contains("\"fetch\""))
            .count()
    }
}

pub(super) fn unbranched() -> GhCloneRepoQuery {
    GhCloneRepoQuery {
        ref_: None,
        ..query()
    }
}

/// Callers of one runtime that share its clone cache: each holds its own
/// token, as the API and git see it.
pub(super) const ALICE: &str = "clone-token-alice";

pub(super) const BOB: &str = "clone-token-bob";

/// One ghCloneRepo row run as the holder of `token`, through the runtime's
/// entry point (`run`), with that token for both the API and git.
pub(super) async fn clone_as(
    bench: &Bench,
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    token: &str,
) -> Result<CloneResult, CloneFailure> {
    let credential = credential(token);
    let request = RequestContext::new(
        octocode_io::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
        Some(credential.clone()),
    );
    run(
        provider,
        query,
        Ok(&request),
        &bench.context_as(Some(&credential)),
    )
    .await
}

/// `commits/{reference}` of the fixture repository answers `token` with
/// `response`.
pub(super) async fn answer_commit(
    server: &wiremock::MockServer,
    reference: &str,
    token: &str,
    response: wiremock::ResponseTemplate,
) {
    use wiremock::matchers::{header, method, path};
    wiremock::Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/fixture-owner/fixture-repo/commits/{reference}"
        )))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(response)
        .mount(server)
        .await;
}

pub(super) fn not_found() -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"}))
}

/// The API paths `server` was asked for, in order.
pub(super) async fn api_calls(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect()
}

pub(super) fn commit(sha: &str) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_string(sha)
}

pub(super) const COMMITS_MAIN: &str = "/api/v3/repos/fixture-owner/fixture-repo/commits/main";
