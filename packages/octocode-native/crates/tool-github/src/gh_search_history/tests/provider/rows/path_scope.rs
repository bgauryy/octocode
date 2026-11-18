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
        assert!(data.get("type").is_none(), "{scope}: {data}");
        assert_eq!(data["commits"][0]["commitSha"], "aaa", "{data}");
        // The listed path is the caller's own input: not restated.
        assert!(data.get("path").is_none(), "{scope}: {data}");
        // The commit read scopes its patches with the published `include`.
        let read = &octocode_runtime_base::test_support::checks::related_call(
            &data,
            "ghGetHistoryItem",
            |input| input["queries"][0]["operation"] == "commit",
        )["input"]["queries"][0];
        assert!(read.get("path").is_none(), "{read}");
        if scope.is_null() {
            assert!(read.get("include").is_none(), "{read}");
        } else {
            let path = scope.as_str().expect("path").trim_end_matches('/');
            assert_eq!(
                read["include"],
                json!([path, format!("{path}/**")]),
                "{read}"
            );
        }
    }
}

/// A path commit listing leads to the first `(#N)` row's PR
/// scoped to that path, and the envelope neither echoes owner/repo nor
/// restates its page facts.
#[tokio::test]
async fn commit_listing_leads_to_the_squash_pull_request() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    let commit = |sha: &str, message: &str| {
        json!({"sha": sha, "commit": {"message": message,
            "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}})
    };
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([commit("aaa", "Bump deps"), commit("bbb", "Fix cache (#9)")]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "number": 3,
                        "title": "Bug",
                        "state": "open",
                        "comments": 4
                    }
                ]
            }"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "path": "src/lib.rs"
        }"#,
    ))
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("commit listing");
    assert!(data["commits"][1].get("prNumber").is_none(), "{data}");
    let read = &octocode_runtime_base::test_support::checks::related_call(
        &data,
        "ghGetHistoryItem",
        |input| input["queries"][0]["operation"] == "pullRequest",
    )["input"]["queries"][0];
    assert_eq!(read["number"], 9, "{data}");
    assert_eq!(
        read["include"],
        json_fixture(r#"["src/lib.rs", "src/lib.rs/**"]"#),
        "{data}"
    );
    assert!(
        data.get("owner").is_none() && data.get("repo").is_none(),
        "{data}"
    );

    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["bug"]
        }"#,
    ))
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("issue search");
    assert!(
        data.get("owner").is_none() && data.get("repo").is_none(),
        "{data}"
    );
    // A comment count is a `fields` pick, not a default row's.
    assert!(data["issues"][0].get("commentsCount").is_none(), "{data}");
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["bug"],
            "fields": ["commentsCount"]
        }"#,
    ))
    .expect("query");
    let picked = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("issue search");
    assert_eq!(picked["issues"][0]["commentsCount"], 4, "{picked}");
}
