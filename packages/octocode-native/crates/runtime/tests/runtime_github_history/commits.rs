//! ghGetHistoryItem commit reads: REST fetch, capped file scans, path-scoped totals and patch views.

use crate::support::{self};
use octocode_test_support::json_fixture;
use serde_json::json;
use support::{Workspace, call, checks, inventory_len, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn gh_get_history_item_commit_fetches_via_rest() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {
                "message": "fix: critical regression",
                "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
            },
            "author": {"login": "alice"},
            "stats": {"additions": 5, "deletions": 2},
            "files": []
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b", "ref": sha}),
    )
    .await
    .expect("gh_get_history_item commit");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["message"], "fix: critical regression", "{data}");
    // A SHA ref is not restated beside the same sha.
    let data = row_data(&outcome);
    assert_eq!(data["commitSha"], sha, "{data}");

    let search =
        json!({"operation":"pullRequest","owner":"a","repo":"b","keywords":[data["commitSha"]]});
    assert_eq!(search["keywords"], json!([sha]), "{data}");
    octocode_runtime_base::contracts::validate("ghSearchHistory", json!({"queries":[search]}))
        .expect("commit identity forms a valid related history search");
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_capped_file_scan_is_not_a_complete_count() {
    // A path-scoped scan that stops at the file-batch cap must not
    // report its file count as complete.
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let commit_path = format!("/api/v3/repos/a/b/commits/{sha}");
    Mock::given(method("GET"))
        .and(path(commit_path.clone()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!("<{}{commit_path}?page=999>; rel=\"next\"", server.uri()).as_str(),
                )
                .set_body_json(json!({
                    "sha": sha,
                    "commit": {
                        "message": "big change",
                        "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
                    },
                    "files": [{"filename": "other/file.rs", "status": "modified",
                               "additions": 1, "deletions": 0}]
                })),
        )
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b", "ref": sha, "include": ["wanted/"]}),
    )
    .await
    .expect("capped commit scan");
    let data = row_data(&outcome);
    // A capped scan states no total for its files, and says why the count
    // is incomplete.
    assert!(
        data.pointer("/sectionPagination/files/total").is_none()
            && data["partialReasons"]
                .as_array()
                .is_some_and(|reasons| !reasons.is_empty()),
        "{}",
        outcome.structured_content
    );
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_ne!(
        data.pointer("/coverage/state")
            .and_then(serde_json::Value::as_str),
        Some("complete"),
        "{data}"
    );
    runtime.close().await;
}

/// A commit's patch view states the whole message: the issue and pull
/// request references a body carries ("closes #2411") are evidence a
/// headline drops, and the row needs no second read to reach them.
#[tokio::test]
async fn commit_patch_view_keeps_the_whole_message() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let message = format!(
        "Fix the parser (#42)\n\n{}\ncloses #2411",
        "Long rationale. ".repeat(400)
    );
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": message, "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "files": [{"filename": "src/a.rs", "status": "modified", "additions": 1, "deletions": 1,
                       "patch": "@@ -1 +1 @@\n-a\n+b"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for sections in [json_fixture(r#"["patches"]"#), json_fixture(r#"["files"]"#)] {
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":sections}),
        )
        .await
        .expect("commit section read");
        let data = row_data(&outcome);
        assert_eq!(data["message"], message, "{sections}: {data}");
        assert!(data.get("messageHeadline").is_none(), "{sections}: {data}");
        assert!(
            data["related"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|call| call["input"]["queries"][0]["operation"] != "commit"),
            "the message is whole, no read restates it: {data}"
        );
    }
    runtime.close().await;
}

/// A path-scoped commit read keeps the whole commit's counts
/// at the top level; the scope's own count is its file page's.
#[tokio::test]
async fn path_scoped_commit_labels_whole_commit_totals() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": "two files", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "stats": {"additions": 30, "deletions": 5, "total": 35},
            "files": [
                {"filename": "src/a.rs", "status": "modified", "additions": 10, "deletions": 5},
                {"filename": "docs/b.md", "status": "modified", "additions": 20, "deletions": 0}
            ]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"include":["src/**"]}),
    )
    .await
    .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFileCount"], 2, "{data}");
    assert_eq!(data["additions"], 30, "{data}");
    assert!(data.get("commitTotals").is_none(), "{data}");
    assert_eq!(inventory_len(&data["files"]), 1, "{data}");
    runtime.close().await;
}

/// Patch gutters number each line on the side of its sign (` `/`+`: the
/// commit, `-`: its first parent); the row's one source read reads the
/// top file's new-side hunk windows at the commit and its old-side hunk
/// windows at the parent, so a gutter number is a direct `ranges` entry on
/// its side.
#[tokio::test]
async fn commit_patch_view_numbers_both_sides_and_reads_each_side() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let parent = "9999999def456abc123def456abc123def456abc";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha, "parents": [{"sha": parent}],
            "commit": {"message": "Fix f", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            // Two changed files: a single-hit row keeps one lead only.
            "files": [{"filename": "src/a.rs", "status": "modified", "additions": 2, "deletions": 1,
                       "patch": "@@ -40,3 +60,4 @@ fn f\n a\n-b\n+c\n+d\n e"},
                      {"filename": "src/b.rs", "status": "modified", "additions": 1, "deletions": 0,
                       "patch": "@@ -1,1 +1,2 @@\n x\n+y"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"]}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -40,3 +60,4 @@ fn f\n60\t a\n41\t-b\n61\t+c\n62\t+d\n63\t e",
        "{data}"
    );
    use base64::Engine as _;
    for (pinned, marker, hunk_line) in [(sha, "new", 61), (parent, "old", 41)] {
        let content = (1..=100)
            .map(|line| format!("{marker} line {line}\n"))
            .collect::<String>();
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/a.rs").await;
        Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
            .and(wiremock::matchers::query_param("ref",pinned))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","path":"src/a.rs","encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(content)})))
            .expect(1).mount(&server).await;
        let ref_ = if marker == "new" {
            &data["commitSha"]
        } else {
            &data["parentCommitShas"][0]
        };
        assert_eq!(ref_, &json!(pinned), "{data}");
        let side = json!({"owner":"a","repo":"b","ref":ref_,"path":data["files"][0]["path"],"ranges":[{"startLine":hunk_line,"endLine":hunk_line}]});
        let read = call(&runtime, "ghGetFileContent", json!({"queries": [side]}))
            .await
            .expect("pinned side read");
        assert_eq!(row_status(&read), "success", "{}", read.structured_content);
        let file = row_data(&read);
        assert!(
            file.to_string()
                .contains(&format!("{marker} line {hunk_line}")),
            "{file}"
        );
    }
    runtime.close().await;
}

/// A patch the output gate redacts says so: the row carries `redacted: true`
/// (as a file read does), no warning, and a clean patch carries neither.
#[tokio::test]
async fn a_redacted_patch_flags_the_row() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let clean = "def456abc123def456abc123def456abc123def4";
    let token = format!("ghp_{}", "a".repeat(37));
    for (commit, patch) in [
        (
            sha,
            format!("@@ -0,0 +1,2 @@\n+[default]\n+token = {token}"),
        ),
        (clean, "@@ -1 +1 @@\n-a\n+b".to_owned()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{commit}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sha": commit,
                "commit": {"message": "add config (#7)", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
                "files": [{"filename": "conf/app.ini", "status": "added", "additions": 2, "deletions": 0,
                           "patch": patch}]
            })))
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read = |commit: &str| json!({"operation":"commit","owner":"a","repo":"b","ref":commit,"sections":["patches"]});
    let outcome = call(&runtime, "ghGetHistoryItem", read(sha))
        .await
        .expect("commit patches");
    let data = row_data(&outcome);
    checks::assert_absent(data, &token);
    assert_eq!(data["redacted"], json!(true), "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
    let outcome = call(&runtime, "ghGetHistoryItem", read(clean))
        .await
        .expect("clean patches");
    assert!(
        row_data(&outcome).get("warnings").is_none(),
        "{}",
        row_data(&outcome)
    );
    runtime.close().await;
}
