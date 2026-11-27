//! A comparison GitHub could not resolve names the side that is missing.
use super::{data, related, served};
use octocode_test_support::json_fixture;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The base is the default branch and resolves; the head does not. The
/// error names the head ref, offers no retry that would compare the default
/// branch with itself, and keeps the refs listing.
#[tokio::test]
async fn a_missing_head_ref_is_named_when_the_base_is_the_default_branch() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "default_branch": "main",
                "full_name": "a/b",
                "private": false,
                "archived": false
            }"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string("a".repeat(40)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-ref-zz9"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json_fixture(
            r#"{"message": "No commit found for SHA: no-such-ref-zz9"}"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/compare/"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let served = served(
        &server,
        json_fixture(
            r#"{
                "operation": "compare",
                "owner": "a",
                "repo": "b",
                "baseRef": "main",
                "headRef": "no-such-ref-zz9"
            }"#,
        ),
    )
    .await;
    let data = data(&served);
    assert_eq!(served["results"][0]["status"], "error", "{served}");
    let error = data["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("\"no-such-ref-zz9\" (headRef) not found"),
        "{served}"
    );
    assert!(!error.contains("(baseRef)"), "{served}");
    assert!(
        related(data, "ghGetHistoryItem").is_empty(),
        "no compare retry: {served}"
    );
    assert!(
        !related(data, "ghStructure").is_empty(),
        "the refs listing stays: {served}"
    );
}
