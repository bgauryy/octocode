//! ghSearchHistory and ghGetHistoryItem (commits, issues, pull requests)
//! through the runtime against a mock GitHub.

use crate::support::{self, Run as _};

use serde_json::json;
use support::{Workspace, call, checks, inventory_len, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[tokio::test]
async fn github_missing_identity_is_a_contract_error() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b"}),
    )
    .await
    .expect_err("missing sha");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_commits_lists_via_rest() {
    let server = MockServer::builder().start().await;
    // Repository metadata (rename) pre-flight
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(&server)
        .await;
    // commit list (no keywords → REST list, not search API)
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "sha": "abc123",
                "commit": {
                    "message": "fix: stabilise parser",
                    "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
                },
                "author": {"login": "alice"}
            }
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "commit", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history commits");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["commits"][0]["messageHeadline"], "fix: stabilise parser",
        "{data}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_issues_list_via_issue_search() {
    // Plain issue listings page `is:issue` search results: GitHub's REST
    // /issues list interleaves pull requests, which left pages short.
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [{"number": 42, "title": "Memory leak in parser", "state": "open",
                       "user": {"login": "alice"}, "labels": []}]
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "issue", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history issues");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["title"], "Memory leak in parser",
        "{data}"
    );
    let requests = server.received_requests().await.expect("recorded");
    let q = requests
        .iter()
        .find(|r| r.url.path() == "/api/v3/search/issues")
        .and_then(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "q")
                .map(|(_, v)| v.into_owned())
        })
        .expect("issue search request");
    assert!(q.contains("repo:a/b") && q.contains("is:issue"), "{q}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_pull_requests_lists_via_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "number": 7,
                "title": "Add concurrency buffering",
                "state": "open",
                "user": {"login": "bob"},
                "head": {"sha": "def456", "ref": "feat/buf"},
                "base": {"sha": "main", "ref": "main"}
            }
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history pullRequests");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["pullRequests"][0]["title"], "Add concurrency buffering",
        "{data}"
    );
    runtime.close().await;
}

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
    assert!(data.get("ref").is_none(), "{data}");
    assert_eq!(
        crate::support::checks::hint(data, "findPullRequest")["query"]["queries"][0]["keywords"],
        json!([sha]),
        "{data}"
    );
    runtime.close().await;
}

/// A squash-merge headline names its pull request: read it directly
/// instead of searching by SHA.
#[tokio::test]
async fn squash_merge_commit_reads_its_pull_request_directly() {
    let server = MockServer::builder().start().await;
    let sha = "30df32a13f9cf5f129b913b967ff6f137c5511d6";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {
                "message": "io: use `spawn_mandatory_blocking` (#8506)\n\nCo-authored-by: x",
                "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
            },
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
    .expect("commit read");
    let data = row_data(&outcome);
    let read = &crate::support::checks::hint(data, "readPullRequest");
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(read["query"]["queries"][0]["number"], 8506, "{data}");
    assert!(
        crate::support::checks::hint(data, "findPullRequest").is_null(),
        "{data}"
    );
    checks::assert_output_contract("ghGetHistoryItem", &outcome.structured_content);
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_issue_fetches_via_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42,
            "title": "Parser OOM on large inputs",
            "state": "open",
            "body": "Detailed reproduction steps here",
            "user": {"login": "alice"},
            "labels": [],
            "comments": 3
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("gh_get_history_item issue");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["title"], "Parser OOM on large inputs",
        "{data}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_preserves_github_permission_reason() {
    let server = MockServer::builder().start().await;
    let leaked_token = format!("ghp_{}", "a".repeat(37));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message": format!("Resource protected by organization SAML SSO authorization; token {leaked_token}")
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("permission error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error");
    assert_eq!(data["httpStatus"], 403);
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|message| message.contains("SAML SSO authorization")),
        "{data}"
    );
    checks::assert_absent(data, &leaked_token);
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_commit_not_found_surfaces_error() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/deadbeef1234567890"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    // Not-found is surfaced as a row-level error (non-panic)
    let result = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b",
               "ref": "deadbeef1234567890"}),
    )
    .await;
    // Either a top-level RuntimeError or a row-level error status
    let is_error = match &result {
        Err(_) => true,
        Ok(outcome) => {
            let status = row_status(outcome);
            status == "error" || status.is_empty()
        }
    };
    assert!(is_error, "expected not-found to surface as error");
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_pull_request_without_content_passes_output_contract() {
    // Regression: the per-row `next` menu omitted required pageSize, so every
    // plain PR fetch tripped outputContractViolation. The summary row carries
    // a body preview: the multibyte body rides the file-list read whole.
    let server = MockServer::builder().start().await;
    let body = "修复并发缓冲区的内存泄漏问题。".repeat(60);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7,
            "title": "Add concurrency buffering",
            "state": "closed",
            "merged_at": null,
            "draft": false,
            "body": body,
            "user": {"login": "bob"},
            "head": {"sha": "def456", "ref": "feat/buf"},
            "base": {"ref": "main"},
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7}),
    )
    .await
    .expect("PR fetch must satisfy the output contract");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let pr = &row_data(&outcome)["pullRequests"][0];
    // The summary previews the body (whole multibyte chars); the file-list
    // read carries it whole.
    assert!(pr.get("body").is_none(), "{pr}");
    let preview = pr["bodyPreview"].as_str().expect("preview");
    assert!(
        preview.starts_with("修复并发") && preview.ends_with('…'),
        "{pr}"
    );
    // The body rides the file-list read.
    let get_body =
        &crate::support::checks::hint(row_data(&outcome), "readFiles")["query"]["queries"][0];
    // Continuations omit defaulted fields; validation restores them on replay.
    assert!(get_body.get("pageSize").is_none(), "{get_body}");
    assert!(get_body.get("minify").is_none(), "{get_body}");
    assert_eq!(get_body["sections"], json!(["body", "files"]), "{get_body}");
    let replayed = octocode_native::contracts::validate_query("ghGetHistoryItem", get_body.clone())
        .expect("compact continuation validates");
    // pageSize has no contract default: each surface sizes its own page.
    assert!(replayed.get("pageSize").is_none(), "{replayed}");
    // History text is always verbatim: no minify field exists.
    assert!(replayed.get("minify").is_none(), "{replayed}");
    runtime.close().await;
}

#[tokio::test]
async fn history_repository_without_owner_is_rejected_before_provider_requests() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "repo": "b"}),
    )
    .await
    .expect_err("repository-only scope must fail native contract validation");
    assert_eq!(outcome.code, "invalidInput");
    let issues = outcome.validation_issues.as_ref().expect("typed issues");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "history.repository-scope");
    assert_eq!(
        serde_json::to_value(&issues[0].path).unwrap(),
        json!(["queries", "0", "owner"])
    );
    assert!(issues[0].message.contains("repo requires owner"));
    let payload = outcome.payload.as_ref().expect("transport error payload");
    for frame in ["kind", "version", "tool"] {
        assert!(payload.get(frame).is_none(), "{frame}: {payload}");
    }
    assert!(
        payload["error"]
            .as_str()
            .is_some_and(|error| error.contains("repo requires owner")),
        "{payload}"
    );
    assert_eq!(payload["errorCode"], "invalidInput");
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
    runtime.close().await;
}

#[tokio::test]
async fn history_rest_page_ceiling_retains_current_items_without_invalid_continuation() {
    for operation in ["pullRequest", "commit"] {
        let server = MockServer::builder().start().await;
        mount_repo_metadata(&server).await;
        let (endpoint, rows, field) = if operation == "pullRequest" {
            (
                "pulls",
                json!([{"number": 9, "title": "Current PR", "state": "open", "user": {"login": "bob"}}]),
                "pullRequests",
            )
        } else {
            (
                "commits",
                json!([{"sha": "abc123", "commit": {"message": "Current commit", "author": {"name": "Bob", "date": "2024-01-01T00:00:00Z"}}, "author": {"login": "bob"}}]),
                "commits",
            )
        };
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/{endpoint}")))
            .and(query_param("page", "1000"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "Link",
                        format!(
                            "<{}/api/v3/repos/a/b/{endpoint}?page=1001>; rel=\"next\"",
                            server.uri()
                        ),
                    )
                    .set_body_json(rows),
            )
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let outcome = call(
            &runtime,
            "ghSearchHistory",
            json!({"operation": operation, "owner": "a", "repo": "b", "page": 1000, "pageSize": 1}),
        )
        .await
        .expect("current page");
        let data = row_data(&outcome);
        assert_eq!(
            data[field].as_array().expect("current rows").len(),
            1,
            "{data}"
        );
        assert_eq!(data["terminalLimit"], true, "{data}");
        assert!(data.pointer("/next/nextPage").is_none(), "{data}");
        runtime.close().await;
    }
}

async fn mount_repo_metadata(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn gh_search_history_pull_request_list_defaults_to_all_states_newest_first() {
    let server = MockServer::builder().start().await;
    mount_repo_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls"))
        .and(query_param("state", "all"))
        .and(query_param("direction", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"number": 9, "title": "Newest", "state": "closed", "user": {"login": "bob"}}
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("PR list");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["pullRequests"][0]["number"], 9, "{data}");
    assert!(data.get("effectiveQuery").is_none(), "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_pull_request_search_works_across_repositories() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [{"number": 3, "title": "Cross repo fix", "state": "open", "user": {"login": "eve"}}]
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "keywords": ["buffer"]}),
    )
    .await
    .expect("cross-repo PR search");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let requests = server.received_requests().await.expect("recorded");
    let q = requests
        .iter()
        .find(|r| r.url.path() == "/api/v3/search/issues")
        .and_then(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "q")
                .map(|(_, v)| v.into_owned())
        })
        .expect("search q");
    assert!(q.contains("is:pr") && !q.contains("repo:"), "{q}");
    assert!(!q.contains("archived:"), "{q}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_commit_list_forwards_committer() {
    let server = MockServer::builder().start().await;
    mount_repo_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("committer", "web-flow"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"sha": "abc", "commit": {"message": "merged via UI", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}}}
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "commit", "owner": "a", "repo": "b", "qualifiers": "committer:web-flow"}),
    )
    .await
    .expect("commit list");
    let data = row_data(&outcome);
    assert_eq!(
        data["commits"][0]["messageHeadline"], "merged via UI",
        "{data}"
    );
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
    assert_eq!(
        data["changedFilesCountScope"], "partial",
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

/// A pull-request read of a number that is an issue offers the issue
/// read instead of a generic not-found.
#[tokio::test]
async fn gh_pull_request_read_of_an_issue_number_offers_read_issue() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 9, "title": "A bug", "state": "open", "user": {"login": "x"}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"extras":[]}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    let read = &crate::support::checks::hint(data, "readIssue")["query"]["queries"][0];
    assert_eq!(read["operation"], "issue", "{data}");
    assert_eq!(read["number"], 9, "{data}");
    assert!(
        data["error"].as_str().unwrap_or("").contains("issue"),
        "{data}"
    );
    runtime.close().await;
}

/// A commit's patch view names the commit by its headline once; the
/// full message is one `readCommit` lead away.
#[tokio::test]
async fn commit_patch_view_carries_the_headline_not_the_whole_message() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let message = format!("Fix the parser (#42)\n\n{}", "Long rationale. ".repeat(400));
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
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"],"extras":["hints"]}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert!(data.get("message").is_none(), "{data}");
    assert_eq!(data["messageHeadline"], "Fix the parser (#42)", "{data}");
    checks::assert_absent(data, "Long rationale");
    let lead = &crate::support::checks::hint(data, "readCommit");
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{data}");
    assert!(
        lead["query"]["queries"][0].get("sections").is_none(),
        "{data}"
    );
    // The pull request the headline names stays a lead beside it: a commit
    // view reads the same discussion route, one file or many.
    assert_eq!(
        crate::support::checks::hint(data, "readPullRequest")["query"],
        json!({"queries":[{"operation":"pullRequest","owner":"a","repo":"b","number":42,"extras":["hints"]}]}),
        "{data}"
    );
    runtime.close().await;
}

/// An issue comment page fits one response page: the comments that
/// fit are shown, and the one comment cursor resumes exactly at the first
/// unshown comment (no second, response-level cursor).
#[tokio::test]
async fn issue_comment_pages_fit_the_response_with_one_cursor() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Big thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 30,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let comments = (0..30)
        .map(|n| json!({"id": n, "user": {"login": "bob"}, "body": format!("comment {n} {}", "x".repeat(900)),
                        "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(comments)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
               "sections":["comments"],"pageSize":30,"extras":[]}],"responseLength":12000}),
    )
    .await
    .expect("issue comments");
    let structured = &outcome.structured_content;
    assert!(
        structured
            .get("responsePagination")
            .is_none_or(|page| page["hasMore"] != true),
        "{structured}"
    );
    let data = row_data(&outcome);
    let shown = data["issues"][0]["comments"].as_array().map_or(0, Vec::len);
    assert!((1..30).contains(&shown), "{shown}: {data}");
    let next = &data["next"]["nextCommentPage"]["query"]["queries"][0];
    let (page, size) = (
        next["page"].as_u64().expect("page"),
        next["pageSize"].as_u64().expect("size"),
    );
    assert_eq!((page - 1) * size, shown as u64, "{data}");
    // Each comment's whole body restates no window.
    checks::assert_absent(data, "bodyPagination");
    runtime.close().await;
}

/// A comment-body continuation hop lists only the
/// bodies it continues, and pages exactly the comments its first window
/// showed. The first window reads the issue body beside the discussion, so
/// its comments get less of the page than a comments-only hop would: the
/// hop must still stop where the first window stopped, or it shows comments
/// first at a later offset (their opening text never read) and its
/// `nextCommentPage` skips them. Every lead of every response is followed;
/// every body (the issue's and each comment's) arrives contiguous and
/// exactly once, and no finished comment is listed again.
#[tokio::test]
async fn issue_comment_body_hops_list_only_unfinished_bodies() {
    let server = MockServer::builder().start().await;
    let issue_body = format!("issue body {}", "b".repeat(9_000));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Long thread", "state": "open", "body": issue_body,
            "user": {"login": "alice"}, "labels": [], "comments": 40,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let body = |n: usize| {
        let size = if n % 2 == 1 { 8_000 } else { 300 };
        format!("comment {n} ") + &format!("{n:02}.").repeat(size / 3)
    };
    let comments = (0..40)
        .map(|n| {
            json!({"id": n, "user": {"login": "bob"}, "body": body(n),
                        "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(comments)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
        "sections":["body","comments"],"length":6000,"extras":["stats"]}]});
    let mut queue = std::collections::VecDeque::from([first]);
    let mut followed = std::collections::HashSet::<String>::new();
    let mut read = std::collections::BTreeMap::<u64, String>::new();
    let mut issue_text = String::new();
    let (mut calls, mut relisted) = (0, 0);
    // One lead offered by two responses runs once, whatever its key order.
    fn canonical(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                let fields = keys
                    .into_iter()
                    .map(|key| format!("{key:?}:{}", canonical(&map[key])))
                    .collect::<Vec<_>>();
                format!("{{{}}}", fields.join(","))
            }
            serde_json::Value::Array(items) => {
                format!(
                    "[{}]",
                    items.iter().map(canonical).collect::<Vec<_>>().join(",")
                )
            }
            scalar => scalar.to_string(),
        }
    }
    while let Some(input) = queue.pop_front() {
        if !followed.insert(canonical(&input)) {
            continue;
        }
        calls += 1;
        assert!(calls <= 60, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                input.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{input}"));
        let content = &outcome.structured_content;
        let data = &content["results"][0]["data"];
        let issue = &data["issues"][0];
        if let Some(text) = issue["body"].as_str() {
            let offset = issue["contentPagination"]["body"]["offset"]
                .as_u64()
                .unwrap_or(0);
            assert_eq!(
                offset as usize,
                issue_text.chars().count(),
                "call {calls}: issue body"
            );
            issue_text.push_str(text);
        }
        for comment in issue["comments"].as_array().into_iter().flatten() {
            let id = match &comment["id"] {
                serde_json::Value::String(id) => id.parse::<u64>().expect("numeric id"),
                id => id.as_u64().expect("id"),
            };
            let text = comment["body"].as_str().unwrap_or("");
            let offset = comment["bodyPagination"]["offset"].as_u64().unwrap_or(0);
            let so_far = read.entry(id).or_default();
            if text.is_empty() && so_far.chars().count() == body(id as usize).chars().count() {
                relisted += 1;
                continue;
            }
            assert_eq!(
                offset as usize,
                so_far.chars().count(),
                "call {calls}: comment {id} {comment}"
            );
            so_far.push_str(text);
        }
        if let Some(next) = content["responsePagination"]["next"]["query"].as_object() {
            queue.push_back(serde_json::Value::Object(next.clone()));
        }
        for (_, lead) in data["next"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(name, _)| {
                matches!(
                    name.as_str(),
                    "continueBody" | "continueCommentBody" | "nextCommentPage"
                )
            })
        {
            queue.push_back(lead["query"].clone());
        }
    }
    assert_eq!(relisted, 0, "finished comments listed again");
    assert_eq!(issue_text, issue_body);
    assert_eq!(read.len(), 40, "{:?}", read.keys().collect::<Vec<_>>());
    for (id, text) in &read {
        assert_eq!(text, &body(*id as usize), "comment {id}");
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
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"include":["src/"],"extras":[]}),
    )
    .await
    .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFilesCount"], 2, "{data}");
    assert_eq!(data["additions"], 30, "{data}");
    assert!(data.get("commitTotals").is_none(), "{data}");
    assert_eq!(inventory_len(&data["files"]), 1, "{data}");
    runtime.close().await;
}

/// An issue read lists the pull requests that closed it
/// (merged first) and offers the merged fix as `next.readPullRequest`; without
/// GraphQL the read falls back to the keyword search hop.
#[tokio::test]
async fn issue_read_lists_closing_pull_requests_and_reads_the_merged_fix() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "model_config mutates", "state": "closed",
            "state_reason": "completed", "body": "repro", "user": {"login": "alice"},
            "labels": [], "comments": 0, "closed_at": "2026-09-25T00:29:37Z",
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:29:37Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {"nodes": [
                {"number": 13794, "state": "CLOSED", "mergedAt": null},
                {"number": 13825, "state": "MERGED", "mergedAt": "2026-09-25T00:29:36Z"}
            ]}}}
        }})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("issue read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let issue = &data["issues"][0];
    assert_eq!(
        issue["closedBy"],
        json!([
            {"number": 13825, "state": "merged", "mergedAt": "2026-09-25T00:29:36Z"},
            {"number": 13794, "state": "closed"}
        ]),
        "{data}"
    );
    let read = &crate::support::checks::hint(data, "readPullRequest");
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(
        read["query"]["queries"][0]["operation"], "pullRequest",
        "{data}"
    );
    assert_eq!(read["query"]["queries"][0]["number"], 13825, "{data}");
    runtime.close().await;

    // GraphQL unavailable: no closedBy, the search hop instead.
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "t", "state": "closed", "state_reason": "completed",
            "body": "", "user": {"login": "alice"}, "labels": [], "comments": 0
        })))
        .mount(&server)
        .await;
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("issue read");
    let data = row_data(&outcome);
    assert!(data["issues"][0].get("closedBy").is_none(), "{data}");
    let find = &crate::support::checks::hint(data, "findPullRequest");
    assert_eq!(find["tool"], "ghSearchHistory", "{data}");
    assert_eq!(
        find["query"]["queries"][0]["keywords"],
        json!(["42"]),
        "{data}"
    );
    runtime.close().await;
}

/// A bounded closing-reference set is disclosed once, on the first body
/// window that lists it; later windows list no `closedBy`, so they spend no
/// GraphQL request on it and restate no bound.
#[tokio::test]
async fn issue_body_windows_keep_closing_reference_coverage() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "synthetic", "state": "closed", "state_reason": "completed",
            "body": "abcdefghijklmno", "user": {"login": "audit"}, "labels": [], "comments": 0,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let nodes = (0..25)
        .map(|n| json!({"number": 101 + n, "state": "CLOSED", "mergedAt": null}))
        .collect::<Vec<_>>();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {
                "totalCount": 31, "pageInfo": {"hasNextPage": true}, "nodes": nodes
            }}}
        }})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut query = json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42,
        "length": 5, "extras": []});
    let mut bodies = Vec::new();
    for window in 0..3 {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .expect("issue window");
        let data = row_data(&outcome);
        bodies.push(
            data["issues"][0]["body"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        );
        let reasons = data["partialReasons"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            reasons.contains(&json!("closingReferenceLimit")),
            window == 0,
            "window {window}: {data}"
        );
        // `next.continueBody` holds the rest: no separate reason restates it.
        assert!(
            !reasons.contains(&json!("contentPagination")),
            "window {window}: {data}"
        );
        let warnings = data["warnings"].to_string();
        assert_eq!(
            warnings.contains("25 of 31"),
            window == 0,
            "window {window}: {data}"
        );
        if window < 2 {
            // `partialReasons` say why the row is partial; the visible
            // body pagination and `next.continueBody` reach the remaining
            // body (no page-note warning restates them).
            assert!(
                data["next"]["continueBody"].is_object(),
                "window {window}: {data}"
            );
            assert_eq!(
                data["issues"][0]["contentPagination"]["body"]["hasMore"],
                json!(true),
                "window {window}: {data}"
            );
            assert!(
                !data["warnings"].to_string().contains("(next.continueBody)"),
                "window {window}: {data}"
            );
        }
        if window == 0 {
            assert_eq!(
                data["issues"][0]["closedBy"].as_array().map(Vec::len),
                Some(25),
                "{data}"
            );
        } else {
            assert!(data["issues"][0].get("closedBy").is_none(), "{data}");
            assert!(
                crate::support::checks::hint(data, "readPullRequest").is_null(),
                "{data}"
            );
        }
        match data["next"]["continueBody"]["query"].as_object() {
            Some(next) => query = serde_json::Value::Object(next.clone()),
            None => assert_eq!(window, 2, "{data}"),
        }
    }
    assert_eq!(bodies, ["abcde", "fghij", "klmno"]);
    let documents = server
        .received_requests()
        .await
        .expect("recorded")
        .into_iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 1, "{documents:?}");
    assert!(documents[0].contains("nodes"), "{documents:?}");
    runtime.close().await;
}

/// Patch gutters number each line on the side of its sign (` `/`+`: the
/// commit, `-`: its first parent); `readAtCommit` reads the top file whole
/// at the commit and `readParent` reads its old-side hunk windows at the
/// parent, so a gutter number is a direct `ranges` entry on its side.
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
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"],"extras":["hints"]}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -40,3 +60,4 @@ fn f\n60\t a\n41\t-b\n61\t+c\n62\t+d\n63\t e",
        "{data}"
    );
    assert_eq!(
        crate::support::checks::hint(data, "readAtCommit")["query"],
        json!({"queries":[{"owner":"a","repo":"b","path":"src/a.rs","ref":sha,"extras":["hints"]}]}),
        "{data}"
    );
    // A lead inherits the row's extras.
    assert_eq!(
        crate::support::checks::hint(data, "readParent")["query"],
        json!({"queries":[{"owner":"a","repo":"b","path":"src/a.rs","ref":parent,"ranges":["30-52"],"extras":["hints"]}]}),
        "{data}"
    );
    runtime.close().await;
}

/// A bogus commit SHA (GitHub 422 "No commit found") is a missing commit:
/// `notFound`, not an invalid query.
#[tokio::test]
async fn bad_commit_sha_is_not_found() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/deadbeef"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: deadbeef"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":"deadbeef","extras":[]}),
    )
    .await
    .expect("commit row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|e| e == "Commit \"deadbeef\" not found in a/b"),
        "{data}"
    );
    // A missed commit leads to a commit search in the repository,
    // never to a refs listing (a SHA-shaped ref names no branch).
    let lead = &crate::support::checks::hint(data, "findCommit");
    assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
    assert_eq!(lead["query"]["queries"][0]["operation"], "commit", "{data}");
    assert!(
        crate::support::checks::hint(data, "viewRefs").is_null(),
        "{data}"
    );
    runtime.close().await;
}

/// GitHub issue comments paged by `per_page`/`page` from one list.
struct CommentList(Vec<serde_json::Value>, String);

impl Respond for CommentList {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };
        let per = param("per_page").unwrap_or(30);
        let page = param("page").unwrap_or(1);
        let start = (page - 1) * per;
        let items = self
            .0
            .iter()
            .skip(start)
            .take(per)
            .cloned()
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200).set_body_json(json!(items));
        if start + per < self.0.len() {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/issues/7/comments?per_page={per}&page={}>; rel=\"next\"",
                    self.1,
                    page + 1
                )
                .as_str(),
            );
        }
        response
    }
}

/// A 105-comment issue thread (two bot comments) walked by its own
/// cursor: every comment, bots included, arrives once, each page's
/// remainder count is exact, and pages fill the response budget (few calls).
#[tokio::test]
async fn issue_comment_walk_reads_every_comment_and_counts_the_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Big thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 105,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let comments = (0..105)
        .map(|n| {
            let login = if n == 40 || n == 90 { "github-actions[bot]" } else { "bob" };
            json!({"id": n, "user": {"login": login}, "body": format!("comment {n} {}", "x".repeat(700)),
                   "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(CommentList(comments, server.uri()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut query = json!({"operation":"issue","owner":"a","repo":"b","number":7,"sections":["comments"],"extras":[]});
    let mut ids = Vec::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 20, "walk did not finish");
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"queries":[query.clone()]}),
        )
        .await
        .expect("comment page");
        let structured = &outcome.structured_content;
        assert!(
            structured.get("responsePagination").is_none(),
            "{structured}"
        );
        let data = row_data(&outcome).clone();
        let issue = &data["issues"][0];
        assert!(issue["comments"].is_array(), "call {calls}: {data}");
        for comment in issue["comments"].as_array().into_iter().flatten() {
            assert!(comment.get("commentType").is_none(), "{comment}");
            assert!(comment.get("updatedAt").is_none(), "{comment}");
            ids.push(comment["id"].as_str().expect("id").to_owned());
        }
        let consumed = ids.len() as u64;
        match data["next"]["nextCommentPage"]["query"]["queries"][0].as_object() {
            Some(next) => {
                // The page position is the visible comment pagination, not a
                // warning line.
                assert_eq!(
                    issue["contentPagination"]["comments"]["hasMore"],
                    json!(true),
                    "consumed {consumed}: {data}"
                );
                assert!(
                    !data["warnings"].to_string().contains("more comments"),
                    "consumed {consumed}: {data}"
                );
                query = serde_json::Value::Object(next.clone());
            }
            None => break,
        }
    }
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!((ids.len(), unique.len()), (105, 105));
    // ~73 KB of comments at a 20k page: budget-filled pages. The resume
    // grid follows the contract's default pageSize (10 since 2026-10-09),
    // so page cuts land one call later than at 30.
    assert!(calls <= 6, "{calls} calls");
    // A page past the discussion's last states it on the comment page
    // block: the page, the page count on its grid, and `outOfRange`.
    let past = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":7,"sections":["comments"],
            "page":40,"pageSize":10,"extras":[]}),
    )
    .await
    .expect("page past the end");
    let data = row_data(&past).clone();
    let block = &data["issues"][0]["contentPagination"]["comments"];
    assert_eq!(block["outOfRange"], true, "{data}");
    assert_eq!(
        (block["currentPage"].clone(), block["totalPages"].clone()),
        (json!(40), json!(11)),
        "{data}"
    );
    assert_eq!(block["hasMore"], false, "{data}");
    assert!(data["issues"][0].get("comments").is_none(), "{data}");
    runtime.close().await;
}

/// A PR summary previews the opening of its verbatim body (an HTML
/// comment is text like any other, never minified away) and leads to the
/// whole body.
#[tokio::test]
async fn pr_summary_previews_the_body() {
    let server = MockServer::builder().start().await;
    let body = format!(
        "<!-- template: describe the change -->\nFixes the cache race.\n\n{}",
        "Details. ".repeat(100)
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Fix cache", "state": "open", "draft": false, "body": body,
            "user": {"login": "bob"}, "head": {"sha": "def456", "ref": "feat"}, "base": {"ref": "main"},
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
            "changed_files": 2, "additions": 3, "deletions": 1
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7}),
    )
    .await
    .expect("PR summary");
    let data = row_data(&outcome);
    let preview = data["pullRequests"][0]["bodyPreview"]
        .as_str()
        .expect("preview");
    assert!(
        preview.starts_with("<!-- template: describe the change --> Fixes the cache race."),
        "{preview}"
    );
    assert!(
        preview.ends_with('…') && preview.chars().count() <= 301,
        "{preview}"
    );
    let leads = crate::support::checks::leads(data);
    assert!(
        leads.iter().any(|lead| lead.query["queries"][0]["sections"]
            .as_array()
            .is_some_and(|sections| sections.contains(&json!("body")))),
        "no whole-body read: {data}"
    );
    runtime.close().await;
}

/// GitHub answers a missing pull request, issue, or comparison ref
/// with a bare 404. The error row must name what was asked for (the number
/// or the compared refs), not the generic "Repository, resource, or path not
/// found"; a comparison also leads to the repository's refs.
#[tokio::test]
async fn gh_history_item_not_found_names_the_item() {
    let server = MockServer::builder().start().await;
    // An existing repository: `main` resolves, `no-such-head` does not (422),
    // and the comparison of an unknown base is GitHub's bare 404.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HISTORY_A_HEAD))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-head"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-head"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (query, named) in [
        (
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":987654}),
            "#987654",
        ),
        (
            json!({"operation":"issue","owner":"a","repo":"b","number":987654}),
            "#987654",
        ),
        (
            json!({"operation":"compare","owner":"a","repo":"b","base":"no-such-base","head":"main"}),
            "no-such-base...",
        ),
        (
            json!({"operation":"compare","owner":"a","repo":"b","base":"main","head":"no-such-head"}),
            "no-such-head",
        ),
    ] {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{query}: {data}");
        let error = data["error"].as_str().unwrap_or_default();
        assert!(error.contains(named), "{query}: {data}");
        if query["operation"] != "compare" || query["head"] == "main" {
            assert!(error.contains("a/b"), "{query}: {data}");
        }
        if query["operation"] == "compare" {
            let lead = &crate::support::checks::hint(data, "viewRefs");
            assert_eq!(lead["tool"], "ghStructure", "{data}");
            assert_eq!(lead["query"]["queries"][0]["operation"], "refs", "{data}");
        }
    }
    runtime.close().await;
}

/// A commit listing GitHub answers with a bare 404 (missing repository
/// or ref) names the repository and ref it listed.
#[tokio::test]
async fn gh_search_history_commit_not_found_names_the_scope() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (query, named) in [
        (
            json!({"operation":"commit","owner":"a","repo":"b","ref":"no-such-ref"}),
            "\"no-such-ref\"",
        ),
        (json!({"operation":"commit","owner":"a","repo":"b"}), "a/b"),
    ] {
        let outcome = call(&runtime, "ghSearchHistory", query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{query}: {data}");
        let error = data["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(named) && error.contains("a/b"),
            "{query}: {data}"
        );
    }
    runtime.close().await;
}

const HISTORY_A_HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

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

/// An issue summary counts its comments and offers the discussion
/// read; an issue without comments offers neither.
#[tokio::test]
async fn history_a_issue_summary_counts_comments_and_offers_the_discussion() {
    for (comments, offered) in [(105, true), (0, false)] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/issues/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "number": 42, "title": "Parser OOM", "state": "open", "body": "repro",
                "user": {"login": "alice"}, "labels": [], "comments": comments,
                "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
            })))
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
        )
        .await
        .expect("issue read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{data}");
        let issue = &data["issues"][0];
        let read = &crate::support::checks::hint(data, "readDiscussion");
        if offered {
            assert_eq!(issue["commentsCount"], 105, "{data}");
            assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
            let q = &read["query"]["queries"][0];
            assert_eq!(q["sections"], json!(["comments"]), "{data}");
            assert_eq!(q["number"], 42, "{data}");
            octocode_native::contracts::validate_query("ghGetHistoryItem", q.clone())
                .expect("the discussion read validates");
        } else {
            assert!(issue.get("commentsCount").is_none(), "{data}");
            assert!(read.is_null(), "{data}");
        }
        runtime.close().await;
    }
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

/// An issue read with its discussion asks for the first comment batch
/// beside the issue itself: the batch depends only on the page asked for,
/// so the read is one round trip, not two.
#[tokio::test]
async fn an_issue_discussion_loads_beside_the_issue() {
    use octocode_github::mock_http::Delayed;
    use std::time::Duration;
    let server = MockServer::builder().start().await;
    let delay = Duration::from_millis(400);
    let issue = Delayed::new(
        delay,
        ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 2,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })),
    );
    let issue_arrivals = issue.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(issue)
        .mount(&server)
        .await;
    let comments = Delayed::new(
        Duration::ZERO,
        ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "user": {"login": "bob"}, "body": "one",
             "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"},
            {"id": 2, "user": {"login": "eve"}, "body": "two",
             "created_at": "2026-09-22T00:00:00Z", "updated_at": "2026-09-22T00:00:00Z"}
        ])),
    );
    let comment_arrivals = comments.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(comments)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":7,
               "sections":["comments"],"extras":[]}),
    )
    .await
    .expect("issue comments");
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["comments"].as_array().map(Vec::len),
        Some(2),
        "{data}"
    );
    let (issue_at, comments_at) = (issue_arrivals.times(), comment_arrivals.times());
    assert_eq!((issue_at.len(), comments_at.len()), (1, 1));
    let gap = comments_at[0]
        .checked_duration_since(issue_at[0])
        .unwrap_or_default();
    assert!(gap < delay / 2, "comments waited {gap:?} for the issue");
    runtime.close().await;
}
