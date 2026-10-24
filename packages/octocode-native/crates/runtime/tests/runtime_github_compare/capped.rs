//! Comparisons past GitHub's file cap: tree-derived paths, include scopes and path-history leads.

use super::{BASE_SHA, HEAD_SHA, mount_capped_compare};
use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A path scope past GitHub's 300-file compare list is not "unchanged": the
/// row warns and leads to the path's commits up to the pinned head.
#[tokio::test]
async fn capped_compare_with_a_path_leads_to_the_path_history() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2","include":["lib/late.rs"]}),
    )
    .await
    .expect("compare");
    let data = row_data(&out).clone();
    assert_eq!(data["files"], json!([]), "{data}");
    let lead = support::checks::related_call(&data, "ghSearchHistory", |input| {
        input["queries"][0]["operation"] == "commit"
    });
    assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("path", "lib/late.rs"))
        .and(query_param("sha", HEAD_SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let scoped = call(&runtime, "ghSearchHistory", lead["input"].clone())
        .await
        .expect("scoped history");
    assert_eq!(
        row_status(&scoped),
        "empty",
        "{}",
        scoped.structured_content
    );
    assert!(
        data["warnings"].to_string().contains("lib/late.rs"),
        "{data}"
    );
    // The range's commit list is not path-scoped: a file read pages it
    // apart (none shown), and its page names each commit by its headline.
    assert!(data.get("commits").is_none(), "{data}");
    assert_eq!(
        data["contentPagination"]["commits"]["unshown"], true,
        "{data}"
    );
    assert!(
        data["contentPagination"]["commits"]
            .get("currentPage")
            .is_none(),
        "{data}"
    );
    let page = data["next"]["nextCommitPage"]["input"].clone();
    let listed = call(&runtime, "ghGetHistoryItem", page)
        .await
        .expect("commit page");
    let commit = &row_data(&listed)["commits"][0];
    assert_eq!(commit["messageHeadline"], "c0", "{commit}");
    assert!(commit.get("message").is_none(), "{commit}");
    runtime.close().await;
}

/// Past GitHub's 300-file compare list the two commits' trees name every
/// other changed path (react v18.2.0...v19.0.0: 300 of thousands): the
/// file list is complete, those rows carry no line counts, and no cap
/// marker or commit-walk lead remains.
#[tokio::test]
async fn capped_compare_reaches_every_changed_path_through_the_trees() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let entry =
        |path: &str, sha: &str| json!({"path":path,"type":"blob","mode":"100644","sha":sha});
    let listed = |sha: &str| {
        (0..300)
            .map(|n| entry(&format!("src/f{n:03}.rs"), sha))
            .collect::<Vec<_>>()
    };
    let mut base = listed(&"4".repeat(40));
    base.push(entry("lib/old.rs", &"5".repeat(40)));
    base.push(entry("lib/same.rs", &"6".repeat(40)));
    let mut head = listed(&"3".repeat(40));
    head.push(entry("lib/new.rs", &"7".repeat(40)));
    head.push(entry("lib/same.rs", &"6".repeat(40)));
    for (sha, tree) in [(BASE_SHA, base), (HEAD_SHA, head)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/git/trees/{sha}")))
            .and(query_param("recursive", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"sha":sha,"truncated":false,"tree":tree})),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2",
               "sections":["files"],"include":["lib/**"]}),
    )
    .await
    .expect("compare past the cap");
    let data = row_data(&read).clone();
    assert_eq!(row_status(&read), "success", "{data}");
    assert_eq!(
        super::file_paths(&data["files"]),
        ["lib/new.rs", "lib/old.rs"]
    );
    assert!(data.get("partialReasons").is_none(), "{data}");
    assert!(data["next"]["listCommits"].is_null(), "{data}");
    assert!(data["warnings"].to_string().contains("trees"), "{data}");
    runtime.close().await;
}

/// `include` scopes a comparison's files like a pull request's; past
/// GitHub's 300-file compare list a scoped path is disclosed as possibly
/// missing, with the path's commit history as the lead.
#[tokio::test]
async fn compare_include_scopes_files_and_capped_scope_leads_to_path_history() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let listed = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2",
               "include":["src/f001.rs","src/f2*.rs"]}),
    )
    .await
    .expect("compare include");
    let data = row_data(&listed).clone();
    assert_eq!(row_status(&listed), "success", "{data}");
    let files = data["files"].to_string();
    assert!(
        files.contains("f001.rs") && files.contains("f200.rs"),
        "{data}"
    );
    assert!(!files.contains("f002.rs"), "{data}");
    // A provider cap leaves the scoped count partial even when the shown
    // inventory fits on one response page.
    assert!(data.get("changedFilesCount").is_none(), "{data}");
    assert_eq!(data["contentPagination"]["files"]["countScope"], "partial");
    assert_eq!(files.matches(".rs").count(), 101, "{data}");
    let beyond = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2",
               "include":["lib/**"]}),
    )
    .await
    .expect("compare include beyond cap");
    let data = row_data(&beyond).clone();
    assert!(
        data["warnings"].to_string().contains("300 changed files"),
        "{data}"
    );
    let lead = support::checks::related_call(&data, "ghSearchHistory", |input| {
        input["queries"][0]["operation"] == "commit"
    });
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("path", "lib/"))
        .and(query_param("sha", HEAD_SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let scoped = call(&runtime, "ghSearchHistory", lead["input"].clone())
        .await
        .expect("scoped history");
    assert_eq!(
        row_status(&scoped),
        "empty",
        "{}",
        scoped.structured_content
    );
    runtime.close().await;
}
