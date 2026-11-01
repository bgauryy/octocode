//! Commit pinning: the commit a ref names now, the default branch and the canonical name key one checkout.
use super::bench::*;
use super::*;

/// A branch that moved since its checkout is checked out again at its new
/// commit, never served stale; the same commit named by its SHA shares the
/// branch's checkout.
#[tokio::test]
async fn checkouts_are_keyed_by_the_commit_the_ref_names_now() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::committing(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
    let first = bench.fixture.first_commit.clone();
    answer_commit(&server, "main", ALICE, commit(&first)).await;
    answer_commit(&server, &first, ALICE, commit(&first)).await;
    let before = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("first clone"));
    let by_sha = clone_as(
        &bench,
        &provider,
        &GhCloneRepoQuery {
            ref_: Some(reference(&first)),
            ..query()
        },
        ALICE,
    )
    .await
    .unwrap_or_else(|_| panic!("clone by SHA"));
    assert_eq!(by_sha.location.local_path, before.location.local_path);
    assert_eq!(by_sha.location.resolved_ref, first);

    let second = bench.fixture.push_second();
    server.reset().await;
    answer_commit(&server, "main", ALICE, commit(&second)).await;
    let after = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("moved branch"));
    assert_eq!(after.location.commit_sha, second);
    assert_ne!(after.location.local_path, before.location.local_path);
}

/// An unbranched request pins the default branch the repository names, and
/// a renamed repository is checked out under its canonical name: the old
/// name's request follows GitHub's redirect.
#[tokio::test]
async fn the_default_branch_and_canonical_name_pin_the_checkout() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
    let redirect = |to: &str| {
        wiremock::ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1{to}", server.uri()),
        )
    };
    for (from, to) in [("", ""), ("/commits/HEAD", "/commits/HEAD")] {
        wiremock::Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/old-owner/fixture-repo{from}")))
            .respond_with(redirect(to))
            .mount(&server)
            .await;
    }
    wiremock::Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"default_branch":"main","full_name":"fixture-owner/fixture-repo"}),
        ))
        .mount(&server)
        .await;
    wiremock::Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/HEAD"))
        .respond_with(commit(&bench.fixture.first_commit))
        .mount(&server)
        .await;
    let renamed = GhCloneRepoQuery {
        owner: "old-owner".parse().expect("owner"),
        ..unbranched()
    };
    let cloned = clone_as(&bench, &provider, &renamed, ALICE)
        .await
        .unwrap_or_else(|failure| panic!("renamed clone: {failure:?}"));
    assert_eq!(cloned.location.resolved_ref, "main");
    assert_eq!(cloned.location.commit_sha, bench.fixture.first_commit);
    assert!(
        cloned.location.local_path.contains("fixture-owner"),
        "{cloned:?}"
    );
    assert_eq!(
        cloned.warnings,
        [
            "Repository old-owner/fixture-repo was renamed to fixture-owner/fixture-repo; leads use the canonical name."
        ],
        "the rename is stated, as the other GitHub tools state it"
    );
}

/// GitHub owner and repository names are case-insensitive: a request that
/// spells them in another case shares the checkout of the same commit.
#[tokio::test]
async fn owner_and_repo_case_variants_share_one_checkout() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(mock_endpoint(&server));
    let provider = one_shot_provider(&server);
    let first = bench.fixture.first_commit.clone();
    answer_commit(&server, "main", ALICE, commit(&first)).await;
    wiremock::Mock::given(method("GET"))
        .and(path(
            "/api/v3/repos/Fixture-Owner/FIXTURE-REPO/commits/main",
        ))
        .respond_with(commit(&first))
        .mount(&server)
        .await;
    let lower = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|failure| panic!("first clone: {failure:?}"));
    let upper = clone_as(
        &bench,
        &provider,
        &GhCloneRepoQuery {
            owner: "Fixture-Owner".parse().expect("owner"),
            repo: "FIXTURE-REPO".parse().expect("repo"),
            ..query()
        },
        ALICE,
    )
    .await
    .unwrap_or_else(|failure| panic!("case variant: {failure:?}"));
    assert_eq!(upper.location.local_path, lower.location.local_path);
    assert!(upper.warnings.is_empty(), "{upper:?}");
}
