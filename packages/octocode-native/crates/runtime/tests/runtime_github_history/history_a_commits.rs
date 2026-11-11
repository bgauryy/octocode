//! History-A regressions: commit and compare views carry the changed
//! files' identity, filter file lists as pull requests do, and lead to the
//! commits a comparison names.

use super::history_a::{history_a_file, history_a_mount_pr};
use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, checks, inventory_len, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An `include` that matches none of a commit's files keeps the
/// whole commit's counts and names the changed directories nearest it.
#[tokio::test]
async fn history_a_commit_include_matching_nothing_warns_with_nearest_dirs() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let files = [
        "docs/a.md",
        "packages/react-dom/src/a.js",
        "packages/react/src/b.js",
        "packages/react/index.js",
        "scripts/x.js",
    ]
    .iter()
    .map(|name| json!({"filename": name, "status": "modified", "additions": 1, "deletions": 0}))
    .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": "five files", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "stats": {"additions": 5, "deletions": 0, "total": 5},
            "files": files
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read = |include: &str| json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"include":[include]});
    let outcome = call(&runtime, "ghGetHistoryItem", read("packages/react/**"))
        .await
        .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFilesCount"], 5, "{data}");
    assert_eq!(inventory_len(&data["files"]), 2, "{data}");
    assert!(data.get("commitTotals").is_none(), "{data}");
    assert!(
        !data["warnings"].to_string().contains("matched 0"),
        "{data}"
    );
    let outcome = call(&runtime, "ghGetHistoryItem", read("packages/reakt/**"))
        .await
        .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFilesCount"], 5, "{data}");
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("matched 0 of 5") && warnings.contains("packages/react/src/"),
        "{data}"
    );
    runtime.close().await;
}

/// A `matchString` with `sections: ["files"]` filters a commit's file list
/// as it filters a pull request's: the files whose patch holds it are
/// listed, and a literal that hits none is an empty answer that says so and
/// leads to the same read without it, not the whole list as if it matched.
#[tokio::test]
async fn history_a_match_string_filters_commit_and_pull_request_file_lists_alike() {
    let server = MockServer::builder().start().await;
    let files = json!([
        history_a_file("src/parser.rs", Some("@@ -1 +1 @@\n-old\n+parsed"), 1),
        history_a_file("src/lexer.rs", Some("@@ -1 +1 @@\n-old\n+lexed"), 1),
    ]);
    history_a_mount_pr(&server, true, files.clone()).await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": "two files", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "stats": {"additions": 2, "deletions": 2, "total": 4},
            "files": files
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let reads = |literal: &str| {
        [
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"sections":["files"],"matchString":[literal]}),
            json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["files"],"matchString":[literal]}),
        ]
    };
    // A pull request row nests its item under `pullRequests`; a commit row is
    // the item itself.
    let item = |read: &serde_json::Value, data: &serde_json::Value| {
        if read["operation"] == "pullRequest" {
            data["pullRequests"][0].clone()
        } else {
            data.clone()
        }
    };
    for read in reads("parsed") {
        let outcome = call(&runtime, "ghGetHistoryItem", read.clone())
            .await
            .expect("read");
        let data = row_data(&outcome);
        let files = &item(&read, data)["files"];
        assert_eq!(inventory_len(files), 1, "{read}\n{data}");
        assert!(files.to_string().contains("parser.rs"), "{read}\n{data}");
        assert!(!files.to_string().contains("lexer.rs"), "{read}\n{data}");
    }
    for read in reads("absent") {
        let outcome = call(&runtime, "ghGetHistoryItem", read.clone())
            .await
            .expect("read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "empty", "{read}\n{data}");
        assert_eq!(
            inventory_len(&item(&read, data)["files"]),
            0,
            "{read}\n{data}"
        );
        assert!(
            data["warnings"].to_string().contains("matched 0 of 2"),
            "{read}\n{data}"
        );
        // The same read without the literal, served as a related call.
        let unfiltered = data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|lead| lead["tool"] == "ghGetHistoryItem")
            .map(|lead| &lead["input"]["queries"][0])
            .any(|query| {
                query["operation"] == read["operation"] && query.get("matchString").is_none()
            });
        assert!(unfiltered, "{read}\n{data}");
    }
    runtime.close().await;
}

/// A comparison lists commit headlines (login first) and
/// one `readCommit` lead for the first commit with more message.
#[tokio::test]
async fn history_a_compare_commits_are_headlines_with_login_authors() {
    let server = MockServer::builder().start().await;
    let head = "1111111111111111111111111111111111111111";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/compare/v1...{head}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ahead", "ahead_by": 3, "behind_by": 0, "total_commits": 3,
            "commits": [
                {"sha": "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1", "author": {"login": "rickhanlonii"},
                 "commit": {"message": "Fix A (#1)", "author": {"name": "Ricky", "date": "2024-01-01T00:00:00Z"}}},
                {"sha": "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2", "author": null,
                 "commit": {"message": "Fix B (#2)\n\nLong body explaining B.", "author": {"name": "Unlinked", "date": "2024-01-02T00:00:00Z"}}},
                {"sha": "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3", "author": {"login": "acdlite"},
                 "commit": {"message": "Fix C\n\nmore", "author": {"name": "Andrew", "date": "2024-01-03T00:00:00Z"}}}
            ],
            "files": [{"filename": "src/a.js", "status": "modified", "additions": 1, "deletions": 0}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":head,
            "sections":["commits"]}),
    )
    .await
    .expect("compare");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let commits = data["commits"].as_array().expect("commits");
    assert_eq!(
        commits
            .iter()
            .map(|c| (c["messageHeadline"].clone(), c["author"].clone()))
            .collect::<Vec<_>>(),
        [
            (json!("Fix A (#1)"), json!("rickhanlonii")),
            (json!("Fix B (#2)"), json!("Unlinked")),
            (json!("Fix C"), json!("acdlite")),
        ],
        "{data}"
    );
    assert!(commits.iter().all(|c| c.get("message").is_none()), "{data}");
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/commits/c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":"c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2","commit":{"message":"Fix B (#2)\n\nLong body explaining B.","author":{"name":"Unlinked","date":"2024-01-02T00:00:00Z"}},"files":[]})))
        .expect(1).mount(&server).await;
    let full = call(
        &runtime,
        "ghGetHistoryItem",
        checks::related_call(data, "ghGetHistoryItem", |input| {
            input["queries"][0]["ref"] == "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2"
        })["input"]
            .clone(),
    )
    .await
    .expect("complete message");
    assert_eq!(
        row_data(&full)["message"],
        "Fix B (#2)\n\nLong body explaining B."
    );
    runtime.close().await;
}
