//! History-A regressions: pull request, commit and compare views carry source, target and merge identity.

use super::HISTORY_A_HEAD;
use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, checks, inventory_len, row_data, row_status};
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

fn history_a_file(name: &str, patch: Option<&str>, additions: u64) -> serde_json::Value {
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

async fn history_a_mount_pr(server: &MockServer, merged: bool, files: serde_json::Value) {
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
        json!({"sections":["patches"],"matchString":"parsed"}),
    ] {
        let mut query =
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"extras":[]});
        for (key, value) in extra.as_object().expect("fields") {
            query[key] = value.clone();
        }
        let outcome = call(&runtime, "ghGetHistoryItem", query)
            .await
            .expect("read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{extra}: {data}");
        let pr = &data["pullRequests"][0];
        assert_eq!(pr["headSha"], HISTORY_A_HEAD, "{extra}: {data}");
        assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "{extra}: {data}");
        // The target-branch tip rides the summary read only.
        assert!(pr.get("baseSha").is_none(), "{extra}: {data}");
        checks::assert_output_contract("ghGetHistoryItem", &outcome.structured_content);
    }
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"extras":[]}),
    )
    .await
    .expect("summary read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "summary: {data}");
    let pr = &data["pullRequests"][0];
    assert_eq!(pr["headSha"], HISTORY_A_HEAD, "summary: {data}");
    assert_eq!(pr["baseSha"], HISTORY_A_BASE, "summary: {data}");
    assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "summary: {data}");
    runtime.close().await;
}

/// A patch read whose page holds a file GitHub sent without a patch
/// reads that file at both sides (its change is in no response), even on a
/// merged PR whose merge read stands for the patched file's new side.
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
            "sections":["patches"],"extras":["hints"]}),
    )
    .await
    .expect("patch read");
    let data = row_data(&outcome);
    // The response's lead cap keeps the head read (and the merge read, by
    // priority); `readParent` is built the same way (unit-tested).
    let head = &crate::support::checks::hint(data, "readAtCommit")["query"]["queries"][0];
    assert_eq!(head["path"], "src/huge.rs", "{data}");
    assert_eq!(head["ref"], HISTORY_A_HEAD, "{data}");
    // The merge read locates the patched file's hunks by text.
    let merge = &crate::support::checks::hint(data, "readAtMerge")["query"]["queries"][0];
    assert_eq!(merge["ref"], HISTORY_A_MERGE, "{data}");
    assert_eq!(
        merge["matchString"],
        json!(["let parsed = parse(input);"]),
        "{data}"
    );
    assert!(merge.get("ranges").is_none(), "{data}");
    runtime.close().await;
}

/// The GraphQL read reads `baseSha` from `baseRefOid` and the
/// commit and review-thread totals; the full header (`extras:["details"]`)
/// carries them, while a plain section read keeps only the identity and the
/// commits code is read at (`headSha`, `mergeCommitSha`).
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
    for (sections, details) in [
        (json!(["body", "reviews"]), true),
        (json!(["body", "files"]), false),
    ] {
        let extras = if details {
            json!(["details"])
        } else {
            json!([])
        };
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
                "sections":sections,"extras":extras}),
        )
        .await
        .expect("GraphQL read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{data}");
        assert!(data.get("graphqlFallback").is_none(), "{data}");
        let pr = &data["pullRequests"][0];
        assert_eq!(pr["headSha"], HISTORY_A_HEAD, "{sections}: {data}");
        assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "{sections}: {data}");
        if details {
            assert_eq!(pr["baseSha"], HISTORY_A_BASE, "{sections}: {data}");
            assert_eq!(pr["commitsCount"], 4, "{data}");
            assert_eq!(pr["reviewThreadsCount"], 2, "{data}");
        } else {
            for dropped in ["baseSha", "commitsCount", "reviewThreadsCount"] {
                assert!(pr.get(dropped).is_none(), "{dropped}: {data}");
            }
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
    let read = |include: &str| json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"include":[include],"extras":[]});
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
        warnings.contains("include matched 0 of 5 changed files; changed directories: packages/react-dom/src/, packages/react/src/, packages/react/, docs/, scripts/."),
        "{data}"
    );
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
                {"sha": "c1", "author": {"login": "rickhanlonii"},
                 "commit": {"message": "Fix A (#1)", "author": {"name": "Ricky", "date": "2024-01-01T00:00:00Z"}}},
                {"sha": "c2", "author": null,
                 "commit": {"message": "Fix B (#2)\n\nLong body explaining B.", "author": {"name": "Unlinked", "date": "2024-01-02T00:00:00Z"}}},
                {"sha": "c3", "author": {"login": "acdlite"},
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
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":head,"extras":["hints"]}),
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
    let read = &crate::support::checks::hint(data, "readCommit")["query"]["queries"][0];
    assert_eq!(read["operation"], "commit", "{data}");
    assert_eq!(read["ref"], "c2", "{data}");
    runtime.close().await;
}
