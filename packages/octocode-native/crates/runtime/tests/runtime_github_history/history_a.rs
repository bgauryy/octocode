//! History-A regressions: pull request views carry source, target and merge
//! identity. Commit and compare views: [`super::history_a_commits`].

use super::HISTORY_A_HEAD;
use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const HISTORY_A_BASE: &str = "89abcdef0123456789abcdef0123456789abcdef";

const HISTORY_A_MERGE: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn history_a_rest_pr(merged: bool) -> serde_json::Value {
    json!({
        "number": 9, "title": "Fix parser", "state": "closed",
        "merged_at": merged.then_some("2024-01-03T00:00:00Z"),
        "merge_commit_sha": HISTORY_A_MERGE, "draft": false, "body": "Fixes it.",
        "user": {"login": "alice"}, "labels": [],
        "head": {"sha": HISTORY_A_HEAD, "ref": "feat"},
        "base": {"sha": HISTORY_A_BASE, "ref": "main"},
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "closed_at": "2024-01-03T00:00:00Z", "comments": 2, "review_comments": 1,
        "commits": 3, "changed_files": 2, "additions": 70000, "deletions": 1
    })
}

pub(super) fn history_a_file(name: &str, patch: Option<&str>, additions: u64) -> serde_json::Value {
    let mut file = json!({
        "sha": "1111111111111111111111111111111111111111", "filename": name,
        "status": "modified", "additions": additions, "deletions": 1,
        "changes": additions + 1
    });
    if let Some(patch) = patch {
        file["patch"] = json!(patch);
    }
    file
}

pub(super) async fn history_a_mount_pr(
    server: &MockServer,
    merged: bool,
    files: serde_json::Value,
) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history_a_rest_pr(merged)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(files))
        .mount(server)
        .await;
}

/// Every PR view names the head and (merged) the merge commit: patch,
/// file-list, and matchString views; the summary also names the recorded
/// target-branch tip.
#[tokio::test]
async fn history_a_every_pr_view_carries_source_target_and_merge_commits() {
    let server = MockServer::builder().start().await;
    history_a_mount_pr(
        &server,
        true,
        json!([
            history_a_file(
                "src/parse.rs",
                Some("@@ -1,2 +1,2 @@\n-let a = 1;\n+let parsed = parse(input);\n ctx"),
                1
            ),
            history_a_file("src/huge.rs", None, 69_999),
        ]),
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for extra in [
        json!({"sections":["patches"]}),
        json!({"sections":["files"]}),
        json!({"sections":["patches"],"matchString": ["parsed"]}),
    ] {
        let mut query = json!({"operation":"pullRequest","owner":"a","repo":"b","number":9});
        for (key, value) in extra.as_object().expect("fields") {
            query[key] = value.clone();
        }
        let outcome = call(&runtime, "ghGetHistoryItem", query)
            .await
            .expect("read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{extra}: {data}");
        let pr = &data["pullRequests"][0];
        assert_eq!(pr["headCommitSha"], HISTORY_A_HEAD, "{extra}: {data}");
        assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "{extra}: {data}");
        assert_eq!(pr["baseCommitSha"], HISTORY_A_BASE, "{extra}: {data}");
        checks::assert_output_contract("ghGetHistoryItem", &outcome.structured_content);
    }
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9}),
    )
    .await
    .expect("summary read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "summary: {data}");
    let pr = &data["pullRequests"][0];
    assert_eq!(pr["headCommitSha"], HISTORY_A_HEAD, "summary: {data}");
    assert_eq!(pr["baseCommitSha"], HISTORY_A_BASE, "summary: {data}");
    assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "summary: {data}");
    runtime.close().await;
}

/// A patch read whose page holds a file GitHub sent without a patch
/// reads that file at both sides because its change is in no response.
/// A separate merge-source read uses the returned merge identity and path.
#[tokio::test]
async fn history_a_unpatched_file_gets_head_and_parent_reads() {
    let server = MockServer::builder().start().await;
    history_a_mount_pr(
        &server,
        true,
        json!([
            history_a_file(
                "src/parse.rs",
                Some("@@ -1,2 +1,2 @@\n-let a = 1;\n+let parsed = parse(input);\n ctx"),
                1
            ),
            history_a_file("src/huge.rs", None, 69_999),
        ]),
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
            "sections":["patches"]}),
    )
    .await
    .expect("patch read");
    let data = row_data(&outcome);
    use base64::Engine as _;
    for (name, path_name, pinned, text) in [
        (
            "readAtCommit",
            "src/huge.rs",
            HISTORY_A_HEAD,
            "huge source\n",
        ),
        (
            "readAtMerge",
            "src/parse.rs",
            HISTORY_A_MERGE,
            "let parsed = parse(input);\n",
        ),
    ] {
        Mock::given(method("GET")).and(path(format!("/api/v3/repos/a/b/contents/{}",path_name.replace('/',"%2F"))))
            .and(wiremock::matchers::query_param("ref",pinned))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","path":path_name,"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(text)})))
            .expect(1).mount(&server).await;
        let read = call(
            &runtime,
            "ghGetFileContent",
            if name == "readAtCommit" {
                data["next"][name]["input"].clone()
            } else {
                assert_eq!(data["pullRequests"][0]["mergeCommitSha"], pinned, "{data}");
                assert!(data["pullRequests"][0]["files"].as_array().expect("files").iter().any(|file| file["path"] == path_name), "{data}");
                json!({"queries":[{"owner":"a","repo":"b","path":path_name,"ref":data["pullRequests"][0]["mergeCommitSha"]}]})
            },
        )
        .await
        .expect("source replay");
        assert_eq!(row_status(&read), "success", "{}", read.structured_content);
        assert!(
            row_data(&read).to_string().contains(text.trim()),
            "{}",
            read.structured_content
        );
    }
    runtime.close().await;
}

/// GraphQL section reads preserve the source identities and provider totals.
#[tokio::test]
async fn history_a_graphql_pr_reads_target_sha_and_totals() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"pullRequest": {
                "number": 7, "title": "Fix parser", "url": "https://x", "state": "MERGED",
                "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
                "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
                "baseRefName": "main", "baseRefOid": HISTORY_A_BASE,
                "headRefName": "feat/parser", "headRefOid": HISTORY_A_HEAD,
                "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
                "closedAt": "2026-01-03T00:00:00Z", "mergedAt": "2026-01-03T00:00:00Z",
                "mergeCommit": {"oid": HISTORY_A_MERGE},
                "comments": {"totalCount": 0}, "changedFiles": 1, "additions": 1, "deletions": 0,
                "commitsCount": {"totalCount": 4}, "reviewThreads": {"totalCount": 2},
                "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"path": "src/a.rs", "additions": 1, "deletions": 0, "changeType": "MODIFIED"}
                ]},
                "reviews": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"databaseId": 11, "author": {"login": "ann"}, "state": "APPROVED",
                     "body": "", "submittedAt": "2026-01-03T00:00:00Z", "commit": {"oid": HISTORY_A_HEAD}}
                ]}
            }}
        }})))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for sections in [json!(["body", "reviews"]), json!(["body", "files"])] {
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
                "sections":sections}),
        )
        .await
        .expect("GraphQL read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{data}");
        let pr = &data["pullRequests"][0];
        assert_eq!(pr["headCommitSha"], HISTORY_A_HEAD, "{sections}: {data}");
        assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "{sections}: {data}");
        assert_eq!(pr["baseCommitSha"], HISTORY_A_BASE, "{sections}: {data}");
        // The totals are the summary read's: a section read names the pull
        // request and its commits only.
        for total in ["commitCount", "reviewThreadsCount"] {
            assert!(pr.get(total).is_none(), "{sections}: {data}");
        }
    }
    let documents = server
        .received_requests()
        .await
        .expect("recorded")
        .into_iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect::<Vec<_>>();
    assert!(documents[0].contains("baseRefOid"), "{documents:?}");
    runtime.close().await;
}
