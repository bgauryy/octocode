//! Access checks: API refusals, and cache hits served only behind the caller's own access check.
use super::bench::*;
use super::*;

/// The pinning `commits/{ref}` answer refuses a row before git runs: a
/// missing ref or repository is not-found input (exit 3) naming what is
/// missing, a rejected credential keeps its provider error, and any other
/// failure (an outage) is returned unclassified for the provider error.
#[test]
fn api_refusals_name_what_is_missing() {
    use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
    let tagged = parse_query(serde_json::json!({
        "owner": "o", "repo": "r", "ref": "nope", "mainGoal": "test", "reasoning": "r"
    }));
    let api = |kind, status: u16, message: &str| ProviderError {
        status: Some(status),
        ..ProviderError::new(kind, message.to_owned())
    };
    let missing_ref = api(
        ProviderErrorKind::Validation,
        422,
        "No commit found for SHA: nope",
    )
    .with_reason(ProviderErrorReason::RefNotFound);
    match api_refusal(&tagged, "nope", missing_ref) {
        Ok(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
            assert!(error.message.contains("\"nope\""), "{error:?}");
            assert!(error.hints[0].contains("ghStructure"), "{error:?}");
            let refs = &error.next.as_ref().expect("refs lead")["viewRefs"];
            assert_eq!(refs["tool"], "ghStructure", "{error:?}");
            assert_eq!(
                refs["query"]["queries"][0],
                serde_json::json!({"owner": "o", "repo": "r", "operation": "refs"}),
                "{error:?}"
            );
        }
        other => panic!("a missing ref is input: {other:?}"),
    }
    let missing_repo = api(ProviderErrorKind::NotFound, 404, "Not Found")
        .with_reason(ProviderErrorReason::RepositoryNotFound);
    match api_refusal(&tagged, "nope", missing_repo) {
        Ok(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound);
            assert_eq!(error.message, "Repository not found: o/r");
            assert_eq!(
                error.next.as_ref().expect("search lead")["findRepository"]["tool"],
                "ghSearchRepo",
                "{error:?}"
            );
        }
        other => panic!("a missing repository is input: {other:?}"),
    }
    for kind in [
        ProviderErrorKind::Authentication,
        ProviderErrorKind::Permission,
    ] {
        assert!(matches!(
            api_refusal(&tagged, "nope", api(kind, 401, "denied")),
            Ok(CloneFailure::Provider(_))
        ));
    }
    assert!(
        api_refusal(
            &tagged,
            "nope",
            api(ProviderErrorKind::Server, 503, "unavailable")
        )
        .is_err()
    );
}

/// A checkout Alice's token cloned is never handed to Bob's token when
/// GitHub denies Bob the repository: Bob gets the not-found a fresh clone
/// would give him, no checkout path, and Alice's checkout stays.
#[tokio::test]
async fn cache_hit_is_not_served_to_a_caller_without_access() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    answer_commit(&server, "main", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(&server, "main", BOB, not_found()).await;

    let alice = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    assert!(!alice.location.cached);
    assert_eq!(alice.location.resolved_ref, "main");
    let clones = bench.clones_seen();

    match clone_as(&bench, &provider, &query(), BOB).await {
        Ok(served) => panic!("Bob was served Alice's checkout: {served:?}"),
        Err(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
            assert_eq!(
                error.message,
                "Repository not found: fixture-owner/fixture-repo"
            );
            test_support::checks::assert_absent(
                &serde_json::json!(error),
                &alice.location.local_path,
            );
        }
        Err(CloneFailure::Provider(error)) => panic!("a denied repository is notFound: {error:?}"),
    }
    assert_eq!(bench.clones_seen(), clones, "Bob ran no git");
    assert!(
        Path::new(&alice.location.local_path)
            .join("README.md")
            .is_file(),
        "Alice's checkout is kept"
    );
    // Each caller pins the commit once, with their own token.
    assert_eq!(api_calls(&server).await, [COMMITS_MAIN, COMMITS_MAIN]);
}

/// A caller GitHub grants the repository is served the hit after one
/// `commits/{ref}` read with their own token, with no re-clone.
#[tokio::test]
async fn cache_hit_is_served_after_the_callers_access_check() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let sha = bench.fixture.first_commit.clone();
    answer_commit(&server, "main", ALICE, commit(&sha)).await;
    answer_commit(&server, "main", BOB, commit(&sha)).await;

    let alice = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    let clones = bench.clones_seen();
    let bob = clone_as(&bench, &provider, &query(), BOB)
        .await
        .unwrap_or_else(|_| panic!("Bob may read the repository"));
    assert!(bob.location.cached, "{bob:?}");
    assert_eq!(bob.location.local_path, alice.location.local_path);
    assert_eq!(bob.location.commit_sha, sha);
    assert_eq!(
        bench.clones_seen(),
        clones,
        "an allowed hit is not re-cloned"
    );
    assert_eq!(api_calls(&server).await, [COMMITS_MAIN, COMMITS_MAIN]);
}

/// An unbranched request checks the caller's access to the default branch
/// it would serve.
#[tokio::test]
async fn the_default_branch_requires_access() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    wiremock::Mock::given(method("GET"))
        .and(path("/api/v3/repos/fixture-owner/fixture-repo"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"default_branch":"main","full_name":"fixture-owner/fixture-repo"}),
        ))
        .mount(&server)
        .await;
    answer_commit(&server, "HEAD", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(&server, "HEAD", BOB, not_found()).await;

    let alice = clone_as(&bench, &provider, &unbranched(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones the default branch"));
    assert_eq!(alice.location.resolved_ref, "main");
    match clone_as(&bench, &provider, &unbranched(), BOB).await {
        Ok(served) => panic!("Bob was served Alice's checkout: {served:?}"),
        Err(CloneFailure::Clone(error)) => assert_eq!(error.code, ErrorCode::NotFound, "{error:?}"),
        Err(CloneFailure::Provider(error)) => panic!("a denied repository is notFound: {error:?}"),
    }
}

/// A check that cannot pass refuses the hit: a caller's spent rate limit
/// never unlocks another caller's checkout.
#[tokio::test]
async fn access_check_failure_fails_closed() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 3600;
    answer_commit(&server, "main", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(
        &server,
        "main",
        BOB,
        wiremock::ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", reset.to_string().as_str())
            .insert_header("x-ratelimit-resource", "core")
            .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
    )
    .await;

    clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    match clone_as(&bench, &provider, &query(), BOB).await {
        Ok(served) => panic!("a rate-limited check served the hit: {served:?}"),
        Err(CloneFailure::Provider(error)) => {
            assert_eq!(error.kind, octocode_github::ProviderErrorKind::RateLimited);
        }
        Err(CloneFailure::Clone(error)) => {
            panic!("a rate limit keeps its provider error: {error:?}")
        }
    }
}

/// An API that never answers refuses the row before git runs.
#[tokio::test]
async fn an_unreachable_api_refuses_the_row_before_git() {
    // An API endpoint nothing listens on: the port is released before use.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port();
    let endpoint = GitHubEndpoint::new(
        url::Url::parse(&format!("http://127.0.0.1:{port}/api/v3")).expect("URL"),
    )
    .expect("endpoint");
    let bench = Bench::at(endpoint.clone());
    let provider = gh_shared_support::one_shot_provider_at(endpoint);
    match clone_as(&bench, &provider, &query(), ALICE).await {
        Ok(served) => panic!("an unreachable API pinned a clone: {served:?}"),
        Err(CloneFailure::Provider(error)) => assert!(
            matches!(
                error.kind,
                octocode_github::ProviderErrorKind::Transport
                    | octocode_github::ProviderErrorKind::Timeout
            ),
            "{error:?}"
        ),
        Err(CloneFailure::Clone(error)) => {
            panic!("a network failure is a provider error: {error:?}")
        }
    }
    assert_eq!(bench.clones_seen(), 0, "no git ran");
}
