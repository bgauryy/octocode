//! The searched repository: renamed ones followed, an inaccessible one stated.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_github::ProviderErrorKind;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A renamed repository is followed, as ghSearchHistory does: the same
/// search (every filter) runs against the canonical name and says so,
/// instead of an empty, partial row with a retry lead. The hit's lines are
/// read at its indexed commit in the canonical repository.
#[tokio::test]
async fn renamed_repository_is_followed_with_every_filter() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", "repo:c/d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"lib.rs","path":"src/lib.rs","sha":"1",
                 "html_url":format!("https://github.com/c/d/blob/{SHA}/src/lib.rs"),
                 "repository":{"full_name":"c/d","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn needle() {}","matches":[{"text":"needle","indices":[3,9]}]}]}
            ]})))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/c/d/contents/src%2Flib.rs",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode("fn needle() {}\n")}),
    )
    .await;
    empty_code_search(&server, true).await;
    renamed_to_c_d(&server).await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["needle"],
                "extensions": ["rs"],
                "path": "src"
            }"#,
        ),
    )
    .await
    .expect("search");
    let data = &out.data;
    assert_ne!(out.status, Some("empty"), "{data}");
    assert_eq!(data["files"][0]["path"], "src/lib.rs", "{data}");
    assert!(data["next"].get("retryRenamed").is_none(), "{data}");
    assert!(data["next"].get("retry").is_none(), "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
    assert_eq!(
        data["warnings"][0],
        octocode_tools_shared::gh_shared::renamed_warning("a", "b", &("c".into(), "d".into())),
        "one shared rename wording: {data}"
    );
    // The rows name the canonical repository (a row, or the page once).
    for (key, canonical) in [("owner", "c"), ("repo", "d")] {
        let stated = data["files"][0].get(key).or_else(|| data.get(key));
        assert_eq!(stated, Some(&json!(canonical)), "{data}");
    }
    let searched = server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, q)| q.into_owned())
        })
        .filter(|q| q.contains("repo:c/d"))
        .collect::<Vec<_>>();
    assert_eq!(searched.len(), 1, "{searched:?}");
    for filter in ["extension:rs", "path:src", "needle"] {
        assert!(searched[0].contains(filter), "{filter}: {searched:?}");
    }
}

/// GitHub answers a code search of a renamed repository's old name with a
/// 422 "cannot be searched" too: that answer is followed like an empty one.
#[tokio::test]
async fn an_unsearchable_renamed_repository_is_followed() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", "repo:c/d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "name": "lib.rs",
                        "path": "src/lib.rs",
                        "sha": "1",
                        "html_url": "https://x",
                        "repository": {
                            "full_name": "c/d",
                            "html_url": "https://x",
                            "url": "https://x"
                        }
                    }
                ]
            }"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", "repo:a/b"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json_fixture(r#"{
            "message": "Validation Failed",
            "errors": [
                {
                    "message": "The listed users and repositories cannot be searched either because the resources do not exist or you do not have permission to view them.",
                    "resource": "Search",
                    "field": "q",
                    "code": "invalid"
                }
            ]
        }"#)))
        .mount(&server)
        .await;
    renamed_to_c_d(&server).await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["needle"],
                "matchIn": ["path"]
            }"#,
        ),
    )
    .await
    .expect("search");
    assert_eq!(out.data["files"][0]["path"], "src/lib.rs", "{}", out.data);
    assert_eq!(
        out.data["warnings"][0],
        octocode_tools_shared::gh_shared::renamed_warning("a", "b", &("c".into(), "d".into())),
        "{}",
        out.data
    );
}

/// An inaccessible repository gets no `retry` for its incomplete
/// result (a retry cannot help) and says why the search is empty.
#[tokio::test]
async fn inaccessible_repository_drops_the_incomplete_retry() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, true).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        404,
        json_fixture(r#"{"message": "Not Found"}"#),
    )
    .await;
    // A missing repository is a not-found failure (never an empty,
    // partial search a retry could fix) that names repository access.
    let error = run(
        &server,
        json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["needle"]}"#),
    )
    .await
    .expect_err("missing repository");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound)
    );
    let failure = octocode_tools_shared::gh_shared::search_failure(error);
    assert!(failure.message.contains("private"), "{}", failure.message);
}
