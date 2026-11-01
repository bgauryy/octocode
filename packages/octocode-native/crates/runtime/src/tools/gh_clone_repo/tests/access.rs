//! Access checks run before every fresh fetch, including replacements.
use super::bench::*;
use super::*;

struct ForbiddenCache;

impl octocode_github::ConditionalCache for ForbiddenCache {
    fn get<'a>(
        &'a self,
        _: &'a octocode_github::Partition,
        _: &'a str,
    ) -> std::pin::Pin<Box<dyn Future<Output = Option<octocode_github::CachedContent>> + Send + 'a>>
    {
        Box::pin(async { panic!("clone read a shared response cache") })
    }

    fn put<'a>(
        &'a self,
        _: &'a octocode_github::Partition,
        _: String,
        _: octocode_github::CachedContent,
        _: octocode_github::Retention,
    ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async { panic!("clone wrote a shared response cache") })
    }
}

#[tokio::test]
async fn clone_bypasses_shared_provider_caches_even_for_full_commit_refs() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = GitHubProvider::new(one_shot_provider(&server).transport, ForbiddenCache);
    let sha = &bench.fixture.first_commit;
    answer_commit(&server, sha, ALICE, commit(sha)).await;
    let query = GhCloneRepoQuery {
        ref_: Some(reference(sha)),
        ..query()
    };
    for _ in 0..2 {
        clone_as(&bench, &provider, &query, ALICE)
            .await
            .expect("uncached clone");
    }
    assert_eq!(api_calls(&server).await.len(), 2);
}

#[tokio::test]
async fn repeated_clone_fetches_fresh_and_writes_no_cache_metadata() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
    answer_commit(&server, "main", ALICE, commit(&bench.fixture.first_commit)).await;
    let first = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .expect("first clone");
    let before = bench.clones_seen();
    let second = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .expect("fresh replacement");
    assert_eq!(first.location.local_path, second.location.local_path);
    assert!(
        bench.clones_seen() > before,
        "each invocation must fetch the checkout again"
    );
    assert!(
        !Path::new(&second.location.local_path)
            .join(".octocode-clone-meta.json")
            .exists()
    );
    let row = serde_json::to_value(second).unwrap();
    assert!(row["location"].get("cached").is_none());
    assert!(row["location"].get("expiresAt").is_none());
}

/// The pinning `commits/{ref}` answer refuses a row before git runs: a
/// missing ref or repository is not-found input (exit 3) naming what is
/// missing, a rejected credential keeps its provider error, and any other
/// failure (an outage) is returned unclassified for the provider error.
#[test]
fn api_refusals_name_what_is_missing() {
    use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
    let tagged = parse_query(serde_json::json!({
        "owner": "o", "repo": "r", "ref": "nope"
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
            let refs = &error.calls.as_ref().expect("refs lead")["viewRefs"];
            assert_eq!(refs["tool"], "ghStructure", "{error:?}");
            assert_eq!(
                refs["input"]["queries"][0],
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
            assert_eq!(
                error.message,
                crate::tools::gh_shared::repository_not_found_message("o", "r")
            );
            assert_eq!(
                error.calls.as_ref().expect("search lead")["findRepository"]["tool"],
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
async fn existing_checkout_is_not_served_to_a_caller_without_access() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
    answer_commit(&server, "main", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(&server, "main", BOB, not_found()).await;

    let alice = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    assert_eq!(alice.location.resolved_ref, "main");
    let clones = bench.clones_seen();

    match clone_as(&bench, &provider, &query(), BOB).await {
        Ok(served) => panic!("Bob was served Alice's checkout: {served:?}"),
        Err(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
            assert_eq!(
                error.message,
                crate::tools::gh_shared::repository_not_found_message(
                    "fixture-owner",
                    "fixture-repo"
                )
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
async fn fresh_clone_follows_the_callers_access_check() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
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
    assert_eq!(bob.location.local_path, alice.location.local_path);
    assert_eq!(bob.location.commit_sha, sha);
    assert_eq!(
        bench.clones_seen(),
        clones + 1,
        "every allowed invocation fetches a fresh checkout"
    );
    assert_eq!(api_calls(&server).await, [COMMITS_MAIN, COMMITS_MAIN]);
}

/// An unbranched request checks the caller's access to the default branch
/// it would serve.
#[tokio::test]
async fn the_default_branch_requires_access() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
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
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
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
    let provider = one_shot_provider_at(endpoint);
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
