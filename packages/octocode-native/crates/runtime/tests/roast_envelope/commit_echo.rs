//! A read pinned to a commit does not repeat that commit as `commitSha`.

use crate::support::{Workspace, call, github_api};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn server() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits/[^/]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "f.txt").await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"hello\n".as_slice(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    server
}

async fn read(server: &MockServer, reference: Option<&str>) -> Value {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(server));
    let mut row = json_fixture(r#"{"owner": "a", "repo": "b", "path": "f.txt"}"#);
    if let Some(reference) = reference {
        row["ref"] = json!(reference);
    }
    let outcome = call(&runtime, "ghGetFileContent", json!({"queries":[row]}))
        .await
        .expect("read");
    runtime.close().await;
    outcome.structured_content.into_value()
}

#[tokio::test]
async fn a_sha_ref_is_not_echoed_but_a_branch_or_default_read_names_its_commit() {
    let server = server().await;
    let pinned = read(&server, Some(SHA)).await;
    assert!(
        pinned["results"][0]["data"].get("commitSha").is_none(),
        "{pinned}"
    );
    assert!(
        pinned["results"][0]["data"]["content"].is_string(),
        "{pinned}"
    );
    for reference in [Some("main"), None] {
        let output = read(&server, reference).await;
        assert_eq!(output["results"][0]["data"]["commitSha"], SHA, "{output}");
    }
}
