//! What a repeat need not ask again besides a success. A
//! renamed repository's route, and a path missing at a commit SHA in a
//! repository that answered. A mutable ref's miss and a missing
//! repository are asked again: either can appear.
use super::*;
use crate::transport::RENAME_ROUTE_WINDOW;

fn at_sha(endpoint: &GitHubEndpoint, git_ref: &str) -> RequestSpec {
    let mut url = endpoint
        .rest(&["repos", "a", "b", "contents", "gone.md"])
        .unwrap();
    url.query_pairs_mut().append_pair("ref", git_ref);
    RequestSpec::get(url)
}

fn repository(endpoint: &GitHubEndpoint) -> RequestSpec {
    RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap())
}

async fn missing_path_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(validated("{\"full_name\":\"a/b\"}", "\"r1\""))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b/contents/gone.md"))
        .respond_with(ResponseTemplate::new(404).set_body_string("{\"message\":\"Not Found\"}"))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_renamed_repositorys_route_is_kept_and_the_rename_stays_visible() {
    let server = MockServer::start().await;
    let sha = "d".repeat(40);
    let new = format!("{}/api/v3/repositories/9/git/blobs/{sha}", server.uri());
    Mock::given(path(format!("/api/v3/repos/old/b/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(301).insert_header("location", new.as_str()))
        .mount(&server)
        .await;
    Mock::given(path(format!("/api/v3/repositories/9/git/blobs/{sha}")))
        .respond_with(validated("{}", "\"v1\""))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(validated("{\"full_name\":\"new/b\"}", "\"m1\""))
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let blob = || {
        RequestSpec::get(
            endpoint
                .rest(&["repos", "old", "b", "git", "blobs", &sha])
                .unwrap(),
        )
    };
    transport.execute(blob(), &request("t")).await.unwrap();
    assert_eq!(
        requests(&server).await.len(),
        2,
        "the redirect and its target"
    );

    let context = request("t");
    let again = transport.execute(blob(), &context).await.unwrap();
    assert_eq!(again.status, 200);
    assert_eq!(again.body.as_ref(), b"{}");
    assert!(
        context.followed_rename("old", "b"),
        "the repeat still names the rename"
    );
    assert!(
        context.reused_since().is_some(),
        "the kept route is disclosed as reused"
    );
    assert_eq!(
        requests(&server).await.len(),
        2,
        "the kept route and pinned answer sent nothing"
    );

    // Another read under the old name goes straight to the id.
    let metadata = RequestSpec::get(endpoint.rest(&["repos", "Old", "B"]).unwrap());
    let context = request("t");
    transport.execute(metadata, &context).await.unwrap();
    assert!(context.followed_rename("Old", "B"));
    let seen = requests(&server).await;
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen[2].url.path(),
        "/api/v3/repositories/9",
        "no redirect hop"
    );

    // Past the route's window the old name is asked again.
    memory.age_all(RENAME_ROUTE_WINDOW + Duration::from_secs(1));
    let context = request("t");
    transport.execute(blob(), &context).await.unwrap();
    assert!(context.followed_rename("old", "b"));
    let seen = requests(&server).await;
    assert_eq!(
        seen[3].url.path(),
        format!("/api/v3/repos/old/b/git/blobs/{sha}")
    );
}

/// A route can go stale (the id gone, the old name reclaimed): when the
/// repository it leads to is missing, the route is forgotten and the given
/// name asked.
#[tokio::test]
async fn a_route_to_a_missing_repository_falls_back_to_the_given_name() {
    let server = MockServer::start().await;
    let new = format!("{}/api/v3/repositories/9", server.uri());
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", new.as_str()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(200).set_body_string("reclaimed"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(ResponseTemplate::new(404).set_body_string("{\"message\":\"Not Found\"}"))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let metadata = || RequestSpec::get(endpoint.rest(&["repos", "old", "b"]).unwrap());
    transport.execute(metadata(), &request("t")).await.unwrap();
    let context = request("t");
    let page = transport.execute(metadata(), &context).await.unwrap();
    assert_eq!(page.body.as_ref(), b"reclaimed");
    assert_eq!(context.reused_since(), None, "the route did not answer");
    transport.execute(metadata(), &request("t")).await.unwrap();
    let seen = requests(&server).await;
    let paths = seen
        .iter()
        .map(|request| request.url.path())
        .collect::<Vec<_>>();
    assert_eq!(
        paths[2..],
        [
            "/api/v3/repositories/9",
            "/api/v3/repos/old/b",
            "/api/v3/repos/old/b",
        ],
        "the forgotten route is not taken again"
    );
}

/// A path missing at a SHA in a renamed repository is that repository's
/// answer: kept once the repository answers under its id, so a repeat
/// sends nothing.
#[tokio::test]
async fn a_path_missing_in_a_renamed_repository_is_kept() {
    let server = MockServer::start().await;
    let sha = "7".repeat(40);
    let new = format!("{}/api/v3/repositories/9", server.uri());
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", new.as_str()))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9/contents/gone.md"))
        .respond_with(ResponseTemplate::new(404).set_body_string("{\"message\":\"Not Found\"}"))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let file = || {
        let mut url = endpoint
            .rest(&["repos", "old", "b", "contents", "gone.md"])
            .unwrap();
        url.query_pairs_mut().append_pair("ref", &sha);
        RequestSpec::get(url)
    };
    let first = request("t");
    let metadata = RequestSpec::get(endpoint.rest(&["repos", "old", "b"]).unwrap());
    transport.execute(metadata, &first).await.unwrap();
    let missed = transport.execute(file(), &first).await.unwrap_err();
    let sent = requests(&server).await.len();
    let context = request("t");
    let again = transport.execute(file(), &context).await.unwrap_err();
    assert_eq!(again, missed);
    assert!(context.followed_rename("old", "b"));
    assert_eq!(requests(&server).await.len(), sent, "nothing was sent");
}

#[tokio::test]
async fn a_kept_route_is_one_credentials_own() {
    let server = MockServer::start().await;
    let new = format!("{}/api/v3/repositories/9", server.uri());
    Mock::given(path("/api/v3/repos/old/b"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", new.as_str()))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repositories/9"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "old", "b"]).unwrap());
    transport.execute(spec(), &request("owner")).await.unwrap();
    transport
        .execute(spec(), &request("stranger"))
        .await
        .unwrap();
    let seen = requests(&server).await;
    assert_eq!(seen.len(), 4, "the stranger followed the redirect itself");
    assert_eq!(seen[2].url.path(), "/api/v3/repos/old/b");
}

#[tokio::test]
async fn a_path_missing_at_a_sha_in_an_answering_repository_is_not_asked_again() {
    let server = missing_path_server().await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "e".repeat(40);
    let first = request("t");
    transport
        .execute(repository(&endpoint), &first)
        .await
        .unwrap();
    let missed = transport
        .execute(at_sha(&endpoint, &sha), &first)
        .await
        .unwrap_err();
    assert_eq!(missed.kind, ProviderErrorKind::NotFound);
    assert_eq!(requests(&server).await.len(), 2);

    let context = request("t");
    let again = transport
        .execute(at_sha(&endpoint, &sha), &context)
        .await
        .unwrap_err();
    assert_eq!(again, missed, "the same failure, typed the same");
    assert_eq!(requests(&server).await.len(), 2, "nothing was sent");
    assert!(
        !context.repository_resolves("a", "b"),
        "a kept miss is no answer from the repository now"
    );
}

#[tokio::test]
async fn a_miss_is_kept_once_its_repository_answers_later_in_the_request() {
    let server = missing_path_server().await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "f".repeat(40);
    let first = request("t");
    transport
        .execute(at_sha(&endpoint, &sha), &first)
        .await
        .unwrap_err();
    transport
        .execute(repository(&endpoint), &first)
        .await
        .unwrap();
    transport
        .execute(at_sha(&endpoint, &sha), &request("t"))
        .await
        .unwrap_err();
    assert_eq!(requests(&server).await.len(), 2, "the repeat sent nothing");
}

#[tokio::test]
async fn misses_that_can_change_are_asked_again() {
    let server = missing_path_server().await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "c".repeat(40);
    // A path at a SHA whose repository never answered: the repository
    // itself may be what is missing.
    for _ in 0..2 {
        transport
            .execute(at_sha(&endpoint, &sha), &request("t"))
            .await
            .unwrap_err();
    }
    assert_eq!(requests(&server).await.len(), 2);
    // A path at a branch: the branch moves.
    for _ in 0..2 {
        let context = request("t");
        transport
            .execute(repository(&endpoint), &context)
            .await
            .unwrap();
        transport
            .execute(at_sha(&endpoint, "main"), &context)
            .await
            .unwrap_err();
    }
    let seen = requests(&server).await;
    let branch_reads = seen
        .iter()
        .filter(|request| request.url.query() == Some("ref=main"))
        .count();
    assert_eq!(branch_reads, 2);
}

#[tokio::test]
async fn a_missing_repository_is_asked_again() {
    let server = MockServer::start().await;
    let sha = "9".repeat(40);
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(404).set_body_string("{\"message\":\"Not Found\"}"))
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    for _ in 0..2 {
        let context = request("t");
        transport
            .execute(repository(&endpoint), &context)
            .await
            .unwrap_err();
        transport
            .execute(at_sha(&endpoint, &sha), &context)
            .await
            .unwrap_err();
    }
    assert_eq!(requests(&server).await.len(), 4);
    assert_eq!(memory.len(), 0, "nothing kept");
}

#[tokio::test]
async fn a_kept_miss_is_one_credentials_own() {
    let server = missing_path_server().await;
    let (transport, endpoint, _) = keeping(&server);
    let sha = "8".repeat(40);
    let owner = request("owner");
    transport
        .execute(repository(&endpoint), &owner)
        .await
        .unwrap();
    transport
        .execute(at_sha(&endpoint, &sha), &owner)
        .await
        .unwrap_err();
    let stranger = request("stranger");
    transport
        .execute(at_sha(&endpoint, &sha), &stranger)
        .await
        .unwrap_err();
    assert_eq!(
        requests(&server).await.len(),
        3,
        "the stranger asked for itself"
    );
}
