//! Empty scoped listings explain the missing history and preserve repair calls.
use super::*;
use octocode_test_support::json_fixture;

/// An empty commit listing scoped to a path broadens by dropping each
/// narrowing filter, the path among them, and leads to the listing that
/// checks the path: its parent directory at the listed ref. An empty
/// pull-request search narrowed by `state` drops it too.
#[tokio::test]
async fn an_empty_path_scoped_listing_broadens_past_the_path_and_checks_it() {
    let server = wiremock::MockServer::builder().start().await;
    octocode_test_support::mock_http::mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([]),
    )
    .await;
    octocode_test_support::mock_http::mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(r#"{"total_count": 0, "incomplete_results": false, "items": []}"#),
    )
    .await;
    let provider = one_shot_provider(&server);
    let run = |row: Value| {
        let provider = &provider;
        async move {
            let query = serde_json::from_value(row).expect("query");
            execute(provider, &query, &default_context(), &Passthrough)
                .await
                .expect("listing")
        }
    };
    let data = run(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "v2",
            "path": "crates/nonexistent",
            "filters": {"committedAt": {"from": "2024-01-01T00:00:00Z"}}
        }"#,
    ))
    .await;
    assert_eq!(data["status"], "empty", "{data}");
    let rows =
        octocode_runtime_base::test_support::checks::related_call(&data, "ghSearchHistory", |_| {
            true
        })["input"]["queries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
    assert!(
        rows.iter()
            .any(|row| row.get("filters").is_none() && row["path"] == "crates/nonexistent"),
        "{data}"
    );
    assert!(
        rows.iter()
            .any(|row| row.get("path").is_none() && row["filters"]["committedAt"].is_object()),
        "{data}"
    );
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(&data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        })["input"]["queries"][0],
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "o",
                "repo": "r",
                "path": "crates",
                "ref": "v2"
            }"#
        ),
        "{data}"
    );
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(&data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        })["tool"],
        "ghStructure"
    );

    let data = run(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "filters": {"isMerged": true}
        }"#,
    ))
    .await;
    let rows = &octocode_runtime_base::test_support::checks::related_call(
        &data,
        "ghSearchHistory",
        |_| true,
    )["input"]["queries"];
    assert!(
        rows.as_array()
            .is_some_and(|rows| rows.iter().any(|row| row.get("state").is_none())),
        "{data}"
    );
    assert!(data["next"].get("viewTree").is_none(), "{data}");
}

/// A path with no commits on GitHub reads as "no history" for a file that
/// only exists in the caller's checkout. The empty answer says what GitHub's
/// history covers (pushed commits at the ref), names the ref and path, and
/// leads to local git history rather than to a wider GitHub search.
#[tokio::test]
async fn an_empty_path_listing_says_unpushed_files_have_no_history_here() {
    let server = wiremock::MockServer::builder().start().await;
    octocode_test_support::mock_http::mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([]),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "v2",
            "path": "src/local_only.rs"
        }"#,
    ))
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("listing");
    assert_eq!(data["status"], "empty", "{data}");
    let hints = data["hints"].to_string();
    assert!(
        hints.contains("src/local_only.rs") && hints.contains("v2"),
        "the hint names the path and ref it searched: {hints}"
    );
    assert!(
        hints.contains("pushed") && hints.contains("local git"),
        "the hint says only pushed history is listed and points at local git: {hints}"
    );
}

/// An empty listing of a path within an author, committer or date window
/// says nothing about the path itself: commits that touch it may lie
/// outside the window. The broadening hint and its path-only search stay.
#[tokio::test]
async fn an_empty_windowed_path_listing_claims_nothing_about_the_path() {
    let server = wiremock::MockServer::builder().start().await;
    octocode_test_support::mock_http::mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([]),
    )
    .await;
    // An empty listing confirms the author it names.
    octocode_test_support::mock_http::mount_json(
        &server,
        "/api/v3/users/ada",
        200,
        json!({"login": "ada"}),
    )
    .await;
    let provider = one_shot_provider(&server);
    for filters in [
        json_fixture(r#"{"committedAt": {"from": "2026-11-18T00:00:00Z"}}"#),
        json_fixture(r#"{"author": "ada"}"#),
    ] {
        let mut row = json_fixture(
            r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "path": "packages/vite/package.json"
            }"#,
        );
        row["filters"] = filters;
        let query = serde_json::from_value(row).expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("listing");
        assert_eq!(data["status"], "empty", "{data}");
        let hints = data["hints"].as_array().cloned().unwrap_or_default();
        assert!(!hints.is_empty(), "the broadening hint stays: {data}");
        assert!(
            hints.iter().all(|hint| !hint
                .as_str()
                .unwrap_or_default()
                .contains("packages/vite/package.json")),
            "no hint makes a claim about the path: {data}"
        );
        let rows = octocode_runtime_base::test_support::checks::related_call(
            &data,
            "ghSearchHistory",
            |_| true,
        )["input"]["queries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            rows.iter()
                .any(|row| row.get("filters").is_none()
                    && row["path"] == "packages/vite/package.json"),
            "the path-only search is offered: {data}"
        );
    }
}
