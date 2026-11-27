//! A materialize that could not write or read what it listed says so in the
//! shape every partial answer has, and a filesystem failure is no
//! configuration problem.
use super::*;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
use serde_json::{Value, json};
use wiremock::MockServer;

const FILES: [&str; 2] = ["src/a.rs", "README.md"];

async fn mount_listing_of_files(server: &MockServer) {
    mount_ref(server, "main").await;
    let tree = FILES
        .iter()
        .map(|path| json!({"path": path, "type": "blob", "size": 12}))
        .chain([json_fixture(r#"{"path": "src", "type": "tree"}"#)])
        .collect::<Vec<_>>();
    mount_json(
        server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha": "c".repeat(40), "truncated": false, "tree": tree}),
    )
    .await;
}

async fn materialize(
    server: &MockServer,
    home: &std::path::Path,
) -> Result<octocode_runtime_base::response::result::ToolData, octocode_github::ProviderError> {
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "ref": "main",
            "maxDepth": 5,
            "materialize": true
        }"#,
    ))
    .expect("tree query");
    crate::gh_structure::execute(
        &one_shot_provider(server),
        &query,
        &default_context(),
        home,
        None,
    )
    .await
}

/// Every file read failed: the checkout lacks text it could name, so the
/// answer is partial in the shape every partial answer has (`isPartial` and
/// a reason), and a call that repeats the materialize leads to the rest.
#[tokio::test]
async fn a_materialize_whose_reads_all_failed_is_partial_with_a_retry() {
    let server = MockServer::builder().start().await;
    mount_listing_of_files(&server).await;
    for file in FILES {
        mount_json(
            &server,
            format!("/api/v3/repos/a/b/contents/{}", file.replace('/', "%2F")),
            500,
            json_fixture(r#"{"message": "Server Error"}"#),
        )
        .await;
    }
    let home = octocode_runtime_base::test_support::Fixture::new();
    let data: Value = materialize(&server, &home.0)
        .await
        .expect("materialize")
        .data;
    // `partialReasons` states the partial checkout once.
    assert!(data["location"].get("coverage").is_none(), "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|r| r == "materializeIncomplete")),
        "{data}"
    );
    let retry = &data["next"]["continueMaterialize"];
    assert_eq!(retry["tool"], "ghStructure", "{data}");
    assert_eq!(retry["input"]["queries"][0]["materialize"], true, "{data}");
}

/// A home the process may not write is a file-access failure of the
/// filesystem (the code the clone path reports), never the `config` refusal
/// that sends a reader to settings.
#[cfg(unix)]
#[tokio::test]
async fn a_materialize_that_cannot_write_is_not_a_configuration_error() {
    use std::os::unix::fs::PermissionsExt;
    let server = MockServer::builder().start().await;
    mount_listing_of_files(&server).await;
    for file in FILES {
        crate::test_support::backed::mount_content(&server, file, "body").await;
    }
    let home = octocode_runtime_base::test_support::Fixture::new();
    std::fs::set_permissions(&home.0, std::fs::Permissions::from_mode(0o555)).expect("lock");
    let writable = std::fs::File::create(home.0.join("probe")).is_ok();
    let result = materialize(&server, &home.0).await;
    std::fs::set_permissions(&home.0, std::fs::Permissions::from_mode(0o755)).expect("unlock");
    if writable {
        return; // a superuser writes anywhere
    }
    let error = result.expect_err("an unwritable home fails the materialize");
    assert_eq!(
        error.code(),
        octocode_contract_types::ErrorCode::FileAccessFailed,
        "{error:?}"
    );
    assert!(!error.retryable, "{error:?}");
}
