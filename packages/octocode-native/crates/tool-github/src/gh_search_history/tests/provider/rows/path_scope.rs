//! Literal path scopes survive the history-to-item handoff.
use super::*;
use octocode_test_support::json_fixture;

/// A commit listing names its row kind like commit search: `type` is
/// `commits`, whether `path` names a file, a directory, or nothing.
#[tokio::test]
async fn commit_listing_rows_are_typed_commits() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json_fixture(
            r#"[
                {
                    "sha": "aaa",
                    "commit": {
                        "message": "Fix cache",
                        "committer": {"date": "2026-01-01T00:00:00Z"}
                    },
                    "author": {"login": "dev"}
                }
            ]"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    for scope in [json!("src/lib.rs"), json!("src/"), Value::Null] {
        let mut query = json_fixture(r#"{"operation": "commit", "owner": "o", "repo": "r"}"#);
        if !scope.is_null() {
            query["path"] = scope.clone();
        }
        let query = serde_json::from_value(query).expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("commit listing");
        assert_eq!(
            data["commitSelection"],
            if scope.is_null() {
                "revisionHistory"
            } else {
                "pathChanges"
            },
            "{data}"
        );
        assert!(data.get("type").is_none(), "{scope}: {data}");
        assert_eq!(
            octocode_test_support::commit_field(&data["commits"][0], "commitSha"),
            "aaa",
            "{data}"
        );
        // The listed path is the caller's own input: not restated.
        assert!(data.get("path").is_none(), "{scope}: {data}");
    }
}
