//! Fail-closed setup: cache-home authorization, path-policy denials, and missing branch, endpoint or git.
use super::*;

/// A runner for paths that must refuse before git runs: invoking it fails
/// the test instead of reaching the network.
struct UnreachableGit;

impl GitRunner for UnreachableGit {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        _control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        panic!("git must not run here: {request:?}")
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        _control: &GitRunControl<'_>,
        _record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        panic!("git must not run here: {request:?}")
    }
}

#[test]
fn creates_a_new_configured_home_before_authorizing_the_clone_target() {
    let fixture = Fixture::shared();
    let root = test_support::Fixture::new();
    let cache_home = root.0.join("new").join("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(fixture.work.clone()),
        additional_roots: vec![cache_home.clone()],
        ..Default::default()
    })
    .expect("policy");
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    assert!(!cache_home.exists());
    let result = clone_pinned(&fixture, &query(), &context).expect("new home clone");
    assert!(cache_home.is_dir());
    assert_eq!(result.location.commit_sha, fixture.first_commit);
    assert!(Path::new(&result.location.local_path).starts_with(cache_home));
}

/// The runtime allows only the checkout trees under the Octocode home
/// (`<home>/tmp/clone`), never the home itself (credentials, logs): a clone
/// must authorize its clones root, not the whole home.
#[test]
fn clones_with_only_the_clones_root_allowed_under_an_existing_home() {
    let fixture = Fixture::shared();
    let (_root, cache_home) = home_root();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(fixture.work.clone()),
        additional_roots: vec![cache::clones_root(&cache_home)],
        ..Default::default()
    })
    .expect("policy");
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let result = clone_pinned(&fixture, &query(), &context).expect("clone under the clones root");
    assert_eq!(result.location.commit_sha, fixture.first_commit);
    assert!(
        Path::new(&result.location.local_path).starts_with(cache::clones_root(&cache_home)),
        "{}",
        result.location.local_path
    );
}

#[test]
fn denied_cache_home_is_not_created() {
    let root = test_support::Fixture::new();
    let outside = test_support::Fixture::new();
    let cache_home = outside.0.join("home");
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = UnreachableGit;
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let error = execute_clone(&query(), &context).expect_err("deny cache home");
    // The policy's own flat code, as the local tools report it.
    assert_eq!(error.code, ErrorCode::OutsideAllowedRoots);
    assert!(!cache_home.exists(), "denied home must not be created");
}

#[test]
fn policy_denials_use_the_codes_the_local_tools_share() {
    use crate::policy::PolicyErrorCode;
    for (code, expected) in [
        (PolicyErrorCode::IgnoredPath, ErrorCode::PathPolicyDenied),
        (PolicyErrorCode::NotFound, ErrorCode::PathNotFound),
        (PolicyErrorCode::InputTooLarge, ErrorCode::FileTooLarge),
        (
            PolicyErrorCode::OutsideAllowedRoots,
            ErrorCode::OutsideAllowedRoots,
        ),
        (
            PolicyErrorCode::SymlinkLoop,
            ErrorCode::PathValidationFailed,
        ),
    ] {
        let error = policy_denied(PolicyError::new(code, "denied"));
        assert_eq!(error.code, expected, "{code:?}");
        assert_eq!(error.message, "denied");
    }
}

#[test]
fn withheld_cache_home_is_denied_as_path_policy() {
    let root = test_support::Fixture::new();
    let cache_home = root.0.join(".ssh").join("home");
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = UnreachableGit;
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let error = execute_clone(&query(), &context).expect_err("withheld cache home");
    assert_eq!(error.code, ErrorCode::PathPolicyDenied, "{error:?}");
    assert!(!cache_home.exists(), "a withheld home must not be created");
}

#[test]
fn missing_branch_endpoint_git_and_portable_paths_fail_closed() {
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = CloneConfig::persistent(&cache_home);
    let missing_git = SystemGit::new(root.0.join("does-not-exist/git"));
    let context = setup(
        &missing_git,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    let pinned = GhCloneRepoQuery {
        ref_: Some(reference(&"a".repeat(40))),
        ..query()
    };
    let error = execute_clone(&pinned, &context).expect_err("missing git");
    assert_eq!(error.code, ErrorCode::GitUnavailable);

    // Plain http is a valid API base, and clone source, on loopback only.
    let loopback = GitHubEndpoint::new(url::Url::parse("http://127.0.0.1/api/v3").expect("URL"))
        .expect("provider endpoint");
    assert_eq!(
        repository_url(&web_root(&loopback).expect("root"), "owner", "repo")
            .expect("loopback clone URL"),
        "http://127.0.0.1/owner/repo.git"
    );
    assert!(
        GitHubEndpoint::new(url::Url::parse("http://ghe.example.com/api/v3").expect("URL"))
            .is_err(),
        "non-loopback http is rejected before any clone URL exists"
    );
    let encoded =
        GitHubEndpoint::new(url::Url::parse("https://example.test/prefix/api/v3").expect("URL"))
            .expect("GHES endpoint");
    assert_eq!(
        repository_url(
            &web_root(&encoded).expect("root"),
            "owner name",
            "repo#name"
        )
        .expect("encoded URL"),
        "https://example.test/prefix/owner%20name/repo%23name.git"
    );
    for sparse_path in [r"..\escape", r"src\portable"] {
        assert!(
            rejects_sparse_path(sparse_path),
            "contract must reject portable sparse path: {sparse_path}"
        );
    }
}
