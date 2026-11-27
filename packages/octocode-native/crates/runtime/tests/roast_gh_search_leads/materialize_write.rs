//! A materialize that cannot write under the Octocode home names the home,
//! not a read permission.
#![cfg(unix)]
use crate::support::{Workspace, call, row_data, row_status};
use octocode_test_support::json_fixture;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

#[tokio::test]
async fn a_materialize_into_a_read_only_home_states_the_write_recovery() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "truncated": false,
            "tree": [{"path":"one.rs","mode":"100644","type":"blob","sha":"1".repeat(40),"size":3}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"type": "file", "encoding": "base64", "content": "b25l"}"#,
        )))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let tmp = workspace.home.join("tmp");
    std::fs::create_dir_all(&tmp).expect("tmp");
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o555)).expect("read-only");
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"materialize":true}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome).clone();
    let served = outcome.structured_content.to_string();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).expect("restore");
    assert_eq!(row_status(&outcome), "error", "{served}");
    assert_eq!(data["errorCode"], "fileAccessFailed", "{served}");
    assert!(served.contains("OCTOCODE_HOME"), "{served}");
    runtime.close().await;
}
