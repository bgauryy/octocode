//! Empty scoped listings explain the missing history and preserve repair calls.
use super::*;

/// An empty commit listing scoped to a path broadens by dropping each
/// narrowing filter, the path among them, and leads to the listing that
/// checks the path: its parent directory at the listed ref. An empty
/// pull-request search narrowed by `state` drops it too.
#[tokio::test]
async fn an_empty_path_scoped_listing_broadens_past_the_path_and_checks_it() {
    let server = wiremock::MockServer::builder().start().await;
    octocode_github::mock_http::mount_json(&server, "/api/v3/repos/o/r/commits", 200, json!([]))
        .await;
    octocode_github::mock_http::mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 0, "incomplete_results": false, "items": []}),
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
    let data = run(json!({"operation":"commit","owner":"o","repo":"r","ref":"v2","path":"crates/nonexistent","filters":{"committedAt":{"from":"2024-01-01T00:00:00Z"}}}))
    .await;
    assert_eq!(data["status"], "empty", "{data}");
    let rows = crate::tools::test_support::checks::related_call(&data, "ghSearchHistory", |_| true)
        ["input"]["queries"]
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
        crate::tools::test_support::checks::related_call(
            &data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["input"]["queries"][0],
        json!({"operation":"tree","owner":"o","repo":"r","path":"crates","ref":"v2"}),
        "{data}"
    );
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["tool"],
        "ghStructure"
    );

    let data = run(json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["fix"],"filters":{"isMerged":true}}))
    .await;
    let rows = &crate::tools::test_support::checks::related_call(&data, "ghSearchHistory", |_| {
        true
    })["input"]["queries"];
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
    octocode_github::mock_http::mount_json(&server, "/api/v3/repos/o/r/commits", 200, json!([]))
        .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(
        json!({"operation":"commit","owner":"o","repo":"r","ref":"v2","path":"src/local_only.rs"}),
    )
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
