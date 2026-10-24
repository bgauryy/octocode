//! Comparison pagination preserves source identity and every requested row.
use super::{BASE_SHA, HEAD_SHA};
use crate::support::{self, Workspace, call, row_data};
use serde_json::json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_compare(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
        .mount(server)
        .await;
    let files = (0..250).map(|n| json!({"filename":format!("f{n:03}.rs"),"status":"modified","additions":1,"deletions":1,"sha":"3".repeat(40)})).collect::<Vec<_>>();
    Mock::given(method("GET")).and(path_regex(format!("^/api/v3/repos/a/b/compare/(v1|{BASE_SHA})\\.\\.\\.{HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "permalink_url":format!("https://github.com/a/b/compare/a:{BASE_SHA}...a:{HEAD_SHA}"),
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
            "commits":[{"sha":HEAD_SHA,"commit":{"message":"change","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],"files":files
        }))).mount(server).await;
}

#[tokio::test]
async fn compare_cursors_deliver_each_requested_file_and_commit_once_in_batches() {
    let server = MockServer::start().await;
    mount_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    for budget in [2500, 12000] {
        let files = json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2","sections":["files"]});
        let commits = json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2","sections":["commits"]});
        let walked = support::walk(
            &runtime,
            "ghGetHistoryItem",
            json!({"queries":[files,commits],"responseChars":budget}),
            |output| {
                output["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|row| {
                        let data = &row["data"];
                        assert!(row["status"] != "error", "{output}");
                        if let Some(head) = data.get("headSha") {
                            assert_eq!(head, HEAD_SHA);
                        }
                        if let Some(base) = data.get("baseSha") {
                            assert_eq!(base, BASE_SHA);
                        }
                        let mut ids = super::file_paths(&data["files"])
                            .into_iter()
                            .map(|path| format!("file:{path}"))
                            .collect::<Vec<_>>();
                        ids.extend(data["commits"].as_array().into_iter().flatten().map(
                            |commit| format!("commit:{}", commit["commitSha"].as_str().unwrap()),
                        ));
                        ids
                    })
                    .collect()
            },
        )
        .await;
        assert_eq!(walked.rows.len(), 251, "budget {budget}: {walked:?}");
        assert!(walked.queries.len() > 1);
    }
    runtime.close().await;
}

#[tokio::test]
async fn compare_file_continuations_reuse_the_pinned_provider_read() {
    let server = MockServer::start().await;
    mount_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let walked = support::walk(&runtime, "ghGetHistoryItem", json!({"queries":[{"operation":"compare","owner":"a","repo":"b","baseRef":BASE_SHA,"headRef":HEAD_SHA,"sections":["files"]}],"responseChars":2500}), |output| {
        output["results"].as_array().into_iter().flatten().flat_map(|row| super::file_paths(&row["data"]["files"])).collect()
    }).await;
    assert_eq!(walked.rows.len(), 250);
    assert!(
        walked
            .queries
            .iter()
            .skip(1)
            .all(|input| input["cursor"].is_string())
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().contains("/compare/"))
            .count(),
        1
    );
    runtime.close().await;
}

/// A compare with an unresolvable ref says "ref" (a branch, tag, or SHA),
/// never GitHub's "No commit found for SHA", and names both refs asked for.
#[tokio::test]
async fn bad_compare_ref_names_refs_not_a_sha() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BASE_SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-tag"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-tag"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"no-such-tag"}),
    )
    .await
    .expect("compare row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    let message = data["error"].as_str().expect("message");
    assert!(!message.contains("No commit found"), "{data}");
    assert!(
        message.contains("v1") && message.contains("no-such-tag"),
        "{data}"
    );
    assert!(message.contains("branch, tag, or SHA"), "{data}");
    runtime.close().await;
}
