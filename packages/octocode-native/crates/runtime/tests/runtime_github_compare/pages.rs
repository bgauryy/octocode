//! Comparison pagination preserves source identity and every requested row.
use super::{BASE_SHA, HEAD_SHA};
use crate::support::{self, Run as _, Workspace, call, row_data};
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

/// Comparison reads made so far: every one names the pinned `BASE...HEAD`.
async fn compare_reads(server: &MockServer) -> usize {
    let requests = server.received_requests().await.unwrap();
    let reads = requests
        .iter()
        .filter(|request| request.url.path().contains("/compare/"))
        .collect::<Vec<_>>();
    for read in &reads {
        assert!(
            read.url
                .path()
                .ends_with(&format!("/compare/{BASE_SHA}...{HEAD_SHA}")),
            "{}",
            read.url
        );
    }
    reads.len()
}

/// Only the first call reads the comparison: a saved response page replays
/// its saved rows, and a file-page continuation answers from the provider
/// evidence its cursor owns, so no continuation reaches GitHub. Every
/// changed file arrives exactly once.
#[tokio::test]
async fn compare_file_continuations_answer_from_cursor_owned_evidence() {
    let server = MockServer::start().await;
    mount_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json!({"queries":[{"operation":"compare","owner":"a","repo":"b","baseRef":BASE_SHA,"headRef":HEAD_SHA,"sections":["files"]}],"responseChars":2500});
    let mut walker = support::checks::Walker::new("ghGetHistoryItem", start);
    let (mut replays, mut continued) = (0, 0);
    while let Some(query) = walker.next_query() {
        let first = query.get("cursor").is_none();
        let before = compare_reads(&server).await;
        let output = runtime
            .run("walk".into(), "ghGetHistoryItem".into(), query.clone())
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"))
            .structured_content
            .into_value();
        let replayed = output["responsePagination"]["replayed"] == true;
        let sent = compare_reads(&server).await - before;
        assert_eq!(sent, usize::from(first), "{query}: {output}");
        if replayed {
            replays += 1;
        } else if !first {
            continued += 1;
        }
        let rows = output["results"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|row| super::file_paths(&row["data"]["files"]))
            .collect();
        walker.accept(support::checks::Page { output, rows });
    }
    let walked = walker.finish();
    assert_eq!(walked.rows.len(), 250);
    assert!(
        walked
            .queries
            .iter()
            .skip(1)
            .all(|input| input["cursor"].is_string())
    );
    assert!(replays > 0, "a saved page replays: {:?}", walked.queries);
    assert!(
        continued > 0,
        "a file-page continuation ran: {:?}",
        walked.queries
    );
    assert_eq!(compare_reads(&server).await, 1);
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
