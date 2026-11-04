//! What stays saved: never a partial, `no-store`, binary or error answer, nor
//! one whose resource is gone; and which reads count as pinned to a commit.
use super::*;

/// A partial answer is worth asking again, however it says so.
#[tokio::test]
async fn a_partial_search_is_never_kept() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":1,"incomplete_results":true,"items":[]}),
        ))
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-github-sso", "partial-results; organizations=7")
                .set_body_json(
                    serde_json::json!({"total_count":1,"incomplete_results":false,"items":[]}),
                ),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    for route in ["issues", "code"] {
        let spec = RequestSpec::get(endpoint.rest(&["search", route]).unwrap());
        transport.execute(spec, &request("t")).await.unwrap();
    }
    assert_eq!(memory.len(), 0);
}

/// `no-store` is the origin's refusal, even for an immutable resource.
#[tokio::test]
async fn no_store_is_never_kept() {
    let server = MockServer::start().await;
    let sha = "c".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-store")
                .set_body_string("{}"),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = RequestSpec::get(
        endpoint
            .rest(&["repos", "a", "b", "commits", &sha])
            .unwrap(),
    );
    transport.execute(spec, &request("t")).await.unwrap();
    assert_eq!(memory.len(), 0);
}

/// Losing the resource or the access to it drops what was kept; a 5xx says
/// nothing about it, so the answer stays.
#[tokio::test]
async fn losing_access_drops_the_saved_answer_and_a_server_error_keeps_it() {
    let server = MockServer::start().await;
    let endpoint = mock_endpoint(&server);
    let quick = RetryPolicy {
        max_attempts: 1,
        ..RetryPolicy::default()
    };
    let mut transport =
        GitHubTransport::new(endpoint.clone(), quick, GitHubBudget::relaxed()).unwrap();
    let memory = Arc::new(Memory::default());
    transport.set_saved_answers(Some(memory.clone()));
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(validated("{}", "\"v1\""))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message":"Not Found"})),
        )
        .mount(&server)
        .await;
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(memory.len(), 1);
    assert!(transport.execute(spec(), &request("t")).await.is_err());
    assert_eq!(memory.len(), 1, "a 5xx is not an answer about the resource");
    let gone = transport.execute(spec(), &request("t")).await.unwrap_err();
    assert_eq!(gone.kind, ProviderErrorKind::NotFound);
    assert_eq!(memory.len(), 0);
}

/// A body that is not text is never kept, as it is never retained.
#[tokio::test]
async fn a_binary_answer_is_not_kept() {
    let server = MockServer::start().await;
    let sha = "f".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xff, 0xfe, 0x00]))
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = RequestSpec::get(
        endpoint
            .rest(&["repos", "a", "b", "git", "blobs", &sha])
            .unwrap(),
    );
    transport.execute(spec, &request("t")).await.unwrap();
    assert_eq!(memory.len(), 0);
}

/// A query bound to a commit answers again unasked; one with errors never
/// stands, and an unbound query is never kept.
#[tokio::test]
async fn only_a_clean_commit_bound_graphql_answer_is_kept() {
    let server = MockServer::start().await;
    let (transport, _, memory) = keeping(&server);
    let sha = "1".repeat(40);
    let dates = serde_json::json!({"data":{"repository":{"object":{
        "history":{"nodes":[{"committedDate":"2026-01-01T00:00:00Z","author":{"name":"n"}}]}
    }}}});
    Mock::given(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":null,"errors":[{"message":"nope"}]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(dates))
        .mount(&server)
        .await;
    let failed = transport
        .path_last_commit("a", "b", &sha, "p", &request("t"))
        .await;
    assert!(failed.is_err());
    assert_eq!(memory.len(), 0, "an answer with errors is not kept");
    let first = transport
        .path_last_commit("a", "b", &sha, "p", &request("t"))
        .await
        .expect("answered");
    assert_eq!(memory.len(), 1);
    let again = transport
        .path_last_commit("a", "b", &sha, "p", &request("t"))
        .await
        .expect("answered");
    assert_eq!(again, first);
    assert_eq!(requests(&server).await.len(), 2, "the repeat sent nothing");
    transport
        .execute_graphql("query{viewer{login}}", serde_json::json!({}), &request("t"))
        .await
        .unwrap();
    assert_eq!(memory.len(), 1, "a mutable query is never kept");
}

/// The classification of a REST read as immutable, on github.com and GHES.
#[test]
fn only_a_read_named_by_a_full_sha_is_pinned() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let pinned = [
        format!("repos/a/b/contents/src/a.rs?ref={sha}"),
        format!("repos/a/b/git/trees/{sha}"),
        format!("repos/a/b/git/trees/{sha}?recursive=1"),
        format!("repos/a/b/git/trees/{sha}:src%2Fa"),
        format!("repos/a/b/git/trees/{sha}%3Asrc%2Fa"),
        format!("repos/a/b/git/blobs/{sha}"),
        format!("repos/a/b/commits/{sha}"),
        format!("repos/a/b/commits/{sha}?page=2&per_page=100"),
        format!("repos/a/b/compare/{sha}...{sha}"),
    ];
    let mutable = [
        "repos/a/b/contents/src/a.rs?ref=main".to_owned(),
        "repos/a/b/contents/src/a.rs".to_owned(),
        format!("repos/a/b/commits/{sha}/pulls"),
        format!("repos/a/b/commits/{sha}/comments"),
        "repos/a/b/commits/main".to_owned(),
        "repos/a/b/commits".to_owned(),
        "repos/a/b/git/trees/main".to_owned(),
        format!("repos/a/b/compare/main...{sha}"),
        format!("repos/a/b/compare/{sha}...main"),
        "repos/a/b".to_owned(),
        "search/code?q=x".to_owned(),
        format!("repos/a/b/pulls/1?ref={sha}"),
    ];
    for root in ["https://api.github.com/", "https://ghe.example/api/v3/"] {
        for (routes, expected) in [(&pinned[..], true), (&mutable[..], false)] {
            for route in routes {
                let url = url::Url::parse(&format!("{root}{route}")).unwrap();
                assert_eq!(
                    RequestSpec::get(url.clone()).pins_for_test(),
                    expected,
                    "{url}"
                );
            }
        }
    }
}
