//! Chained GitHub flows: each lead one call returns must run as written and
//! keep the evidence the previous call found.

use crate::support;

use serde_json::{Value, json};
use support::{Workspace, call, github_api, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BASE_SHA: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FORK_HEAD_SHA: &str = "4444444ddddddddddddddddddddddddddddddddd";

fn commit_row(sha: &str, message: &str) -> Value {
    json!({"sha":sha,"commit":{"message":message,"author":{"name":"a","date":"2024-01-01T00:00:00Z"}}})
}

/// A fork comparison `base...fork:main`: 108 commits at 100 per page, 300 files.
/// The head commit is the last of the range, on page 108 at one per page.
async fn mount_fork_compare(server: &MockServer) {
    let files = (0..300)
        .map(|n| json!({"filename":format!("src/f{n:03}.rs"),"status":"modified","additions":1,"deletions":1,"sha":"3".repeat(40)}))
        .collect::<Vec<_>>();
    let body = |commits: Vec<Value>, files: Vec<Value>| {
        json!({
            "status":"ahead","ahead_by":108,"behind_by":2,"total_commits":108,
            "permalink_url":"https://github.com/a/b/compare/a:1111111...fork:4444444",
            "base_commit":{"sha":BASE_SHA},
            "merge_base_commit":{"sha":BASE_SHA,"commit":{"committer":{"date":"2024-03-01T10:00:00Z"}}},
            "commits":commits,"files":files
        })
    };
    let first = (0..100)
        .map(|n| commit_row(&format!("{:040x}", 100 + n), &format!("c{n}")))
        .collect();
    Mock::given(method("GET"))
        .and(path_regex(
            "^/api/v3/repos/a/b/compare/main\\.\\.\\.fork(:|%3A)main$",
        ))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body(first, files)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(
            "^/api/v3/repos/a/b/compare/main\\.\\.\\.fork(:|%3A)main$",
        ))
        .and(query_param("page", "108"))
        .and(query_param("per_page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(body(vec![commit_row(FORK_HEAD_SHA, "last")], Vec::new())),
        )
        .mount(server)
        .await;
}

/// A cross-fork head `owner:branch` moves; the row pins it to the
/// range's last commit, and every continuation names that SHA, not the
/// branch (a ref the upstream repository cannot resolve).
#[tokio::test]
async fn a_cross_fork_compare_pins_its_head_to_a_commit() {
    let server = MockServer::builder().start().await;
    mount_fork_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let out = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"main","headRef":"fork:main",
               "include":["lib/**"]}),
    )
    .await
    .expect("fork compare");
    let data = row_data(&out).clone();
    assert_eq!(row_status(&out), "success", "{data}");
    assert_eq!(data["headSha"], FORK_HEAD_SHA, "{data}");
    let narrow = support::checks::related_call(&data, "ghSearchHistory", |input| {
        input["queries"][0]["operation"] == "commit" && input["queries"][0]["pathPrefix"] == "lib/"
    });
    assert_eq!(narrow["kind"], "related", "{data}");
    assert_eq!(narrow["input"]["queries"][0]["ref"], FORK_HEAD_SHA);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("sha", FORK_HEAD_SHA))
        .and(query_param("path", "lib/"))
        .and(query_param("since", "2024-03-01T10:00:00Z"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([commit_row(FORK_HEAD_SHA, "scoped change")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let scoped = call(&runtime, "ghSearchHistory", narrow["input"].clone())
        .await
        .expect("pinned scope continuation");
    assert_eq!(row_status(&scoped), "success", "{}", row_data(&scoped));
    assert_eq!(row_data(&scoped)["commits"][0]["commitSha"], FORK_HEAD_SHA);
    runtime.close().await;
}

fn commit_with_files(sha: &str, message: &str) -> Value {
    json!({
        "sha": sha,
        "commit": {"message": message, "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}},
        "parents": [{"sha": "5555555eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}],
        "stats": {"additions": 2, "deletions": 0},
        "files": [
            {"filename":"lib/a.js","status":"modified","additions":1,"deletions":0,"patch":"@@ -1,1 +1,2 @@\n x\n+y"},
            {"filename":"lib/b.js","status":"modified","additions":1,"deletions":0,"patch":"@@ -1,1 +1,2 @@\n x\n+z"}
        ]
    })
}

/// The commit→PR lead survives the lead cap beside the commit's own
/// re-read (ahead of the diff's side reads), whether the PR is named by a
/// `(#N)` headline or a `PR-URL:` trailer.
#[tokio::test]
async fn a_commit_patch_read_keeps_its_pull_request_lead() {
    let server = MockServer::builder().start().await;
    let squash = "30df32a13f9cf5f129b913b967ff6f137c5511d6";
    let trailer = "464cbbda0000000000000000000000000000000a";
    for (sha, message) in [
        (squash, "io: spawn (#8506)\n\nWhy: a reason"),
        (
            trailer,
            "url: fix\n\nPR-URL: https://github.com/a/b/pull/65801\nReviewed-By: x",
        ),
    ] {
        Mock::given(method("GET"))
            .and(path_regex(format!("^/api/v3/repos/a/b/commits/{sha}$")))
            .respond_with(ResponseTemplate::new(200).set_body_json(commit_with_files(sha, message)))
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    for (sha, number) in [(squash, 8506), (trailer, 65801)] {
        let out = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation":"commit","owner":"a","repo":"b","ref":sha,
                   "sections":["patches"]}),
        )
        .await
        .expect("commit patches");
        let data = row_data(&out).clone();
        let read = &crate::support::checks::find_related_call(&data, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "pullRequest"
        })
        .unwrap_or(&serde_json::Value::Null);
        assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
        assert_eq!(read["input"]["queries"][0]["number"], number, "{data}");
        assert!(
            crate::support::checks::find_related_call(
                &data,
                "ghSearchHistory",
                |input| input["queries"][0]["operation"] == "pullRequest"
            )
            .unwrap_or(&serde_json::Value::Null)
            .is_null(),
            "{data}"
        );
    }
    runtime.close().await;
}

/// A comparison cut at GitHub's 300-file cap says so, without any
/// `include`, and still leads to every changed file.
#[tokio::test]
async fn a_capped_compare_states_the_file_cap() {
    let server = MockServer::builder().start().await;
    mount_fork_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let out = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"main","headRef":"fork:main",
               "sections":["files"]}),
    )
    .await
    .expect("capped compare");
    let data = row_data(&out).clone();
    assert!(
        data["warnings"].to_string().contains("at most 300"),
        "{data}"
    );
    assert_eq!(
        data["partialReasons"],
        json!(["providerFileLimit"]),
        "{data}"
    );
    assert!(!data["next"]["listCommits"].is_null(), "{data}");
    runtime.close().await;
}

/// A directory at GitHub's 1000-entry Contents limit is complete only
/// in a checkout; the listing leads to the clone of that directory.
#[tokio::test]
async fn a_directory_at_the_contents_limit_leads_to_a_clone() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let entries = (0..1000)
        .map(|n| json!({"name":format!("f{n}.rs"),"path":format!("f{n}.rs"),"type":"file","size":1,"sha":"1".repeat(40)}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(entries))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let out = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner":"a","repo":"b"}),
    )
    .await
    .expect("listing");
    let data = row_data(&out).clone();
    assert_eq!(
        data["providerLimit"]["maxEntriesPerDirectory"], 1000,
        "{data}"
    );
    let clone = &crate::support::checks::find_related_call(&data, "ghCloneRepo", |_| true)
        .unwrap_or(&serde_json::Value::Null);
    assert_eq!(clone["tool"], "ghCloneRepo", "{data}");
    let query = &clone["input"]["queries"][0];
    assert_eq!(query["owner"], "a", "{data}");
    assert_eq!(query["ref"], sha, "{data}");
    runtime.close().await;
}

/// GitHub code search answers a login that does not exist with an
/// empty, incomplete page; the empty owner-wide search checks the login and
/// reports it missing instead of offering a retry.
#[tokio::test]
async fn an_unknown_owner_is_not_found_not_an_empty_search() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/search/code$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":true,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/(users|orgs)/nobody/repos$"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let out = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"nobody","keywords":["foo"]}),
    )
    .await
    .expect("unknown owner");
    let data = row_data(&out).clone();
    assert_eq!(row_status(&out), "error", "{data}");
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["error"].as_str().unwrap_or("").contains("\"nobody\""),
        "{data}"
    );
    assert!(data.get("next").is_none(), "{data}");
    assert!(
        !data["hints"].to_string().contains("unproven absence"),
        "{data}"
    );
    runtime.close().await;
}

/// A complete comparison whose `include` matches none of its files is an
/// empty answer, as it is on a commit, not a bare `files: []` success.
#[tokio::test]
async fn a_compare_scope_that_matches_no_file_is_empty() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/v3/repos/a/b/compare/{BASE_SHA}\\.\\.\\.{FORK_HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},
            "merge_base_commit":{"sha":BASE_SHA,"commit":{"committer":{"date":"2024-03-01T10:00:00Z"}}},
            "commits":[commit_row(FORK_HEAD_SHA, "c")],
            "files":[{"filename":"src/a.rs","status":"modified","additions":1,"deletions":0}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let out = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":BASE_SHA,"headRef":FORK_HEAD_SHA,
               "sections":["files"],"include":["docs/**"]}),
    )
    .await
    .expect("compare");
    let data = row_data(&out).clone();
    assert_eq!(row_status(&out), "empty", "{data}");
    assert!(
        data["warnings"].to_string().contains("matched 0 of 1"),
        "{data}"
    );
    runtime.close().await;
}
