//! A renamed repository: the row and every lead name it as it is now.
use super::*;
use octocode_tools_shared::gh_shared::test_support::{assert_leads_canonical, mount_renamed};

/// The row of a read GitHub answered through a rename redirect, as the
/// tool returns it (its leads on their public channels).
async fn read_renamed(server: &MockServer, row: Value) -> Value {
    let provider = mock_provider(server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(row).expect("valid row");
    execute(&provider, &request, &context, &Passthrough)
        .await
        .expect("history item")
        .data
}

#[track_caller]
fn assert_names_the_rename(data: &Value) {
    assert_eq!(
        (data["owner"].as_str(), data["repo"].as_str()),
        (Some("c"), Some("d")),
        "{data}"
    );
    assert_eq!(
        data["warnings"],
        json!([octocode_tools_shared::gh_shared::renamed_warning(
            "a",
            "b",
            &("c".into(), "d".into())
        )]),
        "{data}"
    );
    assert_leads_canonical(data);
}

/// A commit read under the old name leads (its files' reads) under the
/// canonical one: each lead runs unchanged.
#[tokio::test]
async fn a_commit_of_a_renamed_repository_leads_canonically() {
    let server = MockServer::builder().start().await;
    mount_renamed(&server).await;
    let mut raw = commit_raw(HEAD, "Fix the reader (#12)");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["files"] = json!([changed_file("src/a.rs")]);
    mount_json(
        &server,
        format!("/api/v3/repositories/1/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read_renamed(
        &server,
        json!({"operation":"commit","owner":"a","repo":"b","ref":HEAD,"sections":["patches"]}),
    )
    .await;
    assert_eq!(out["commitSha"], HEAD, "{out}");
    assert_names_the_rename(&out);
}

/// A comparison read under the old name leads (the changed file's read,
/// its commits) under the canonical one. A file filter pages the commits
/// apart, so the read has a commits lead to check.
#[tokio::test]
async fn a_comparison_of_a_renamed_repository_leads_canonically() {
    let server = MockServer::builder().start().await;
    mount_renamed(&server).await;
    mount_json(
        &server,
        format!("/api/v3/repositories/1/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":BASE},
            "commits":[commit_raw(HEAD, "Add feature")],
            "files":[changed_file("packages/x/y.json")]}),
    )
    .await;
    let out = read_renamed(
        &server,
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":BASE,"headRef":HEAD,
            "minChanges":1}),
    )
    .await;
    assert_eq!(out["headCommitSha"], HEAD, "{out}");
    assert_names_the_rename(&out);
}

/// A repository GitHub never redirected keeps its name and states no
/// rename, and its metadata is never read for one.
#[tokio::test]
async fn a_repository_that_stands_states_no_rename() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix the reader (#12)");
    raw["files"] = json!([changed_file("src/a.rs")]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD}),
    )
    .await;
    assert!(out.get("warnings").is_none(), "{out}");
    let requests = server.received_requests().await.expect("requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/api/v3/repos/o/r"),
        "{requests:?}"
    );
}
