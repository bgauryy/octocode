//! Saved GitHub pages: snapshot replay at a ref, credential checks, and history-item pages after a restart.

use super::{SHA, lines};
use crate::support::{self, Run as _, Workspace};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Dispatch counter: the content endpoint is hit `content_hits` times.
async fn github_file_server(content_hits: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    let sha = SHA;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let body = lines("remote");
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(&body),
            "size": body.len(),
            "sha": "f".repeat(40),
            "path": "src/lib.rs"
        })))
        .expect(content_hits)
        .mount(&server)
        .await;
    server
}

/// A matching snapshot replays the stored answer at a commit SHA and on a
/// branch alike. A mismatched snapshot fails without fetching content again.
#[tokio::test]
async fn a_matching_snapshot_replays_and_a_mismatch_fails_without_dispatch() {
    for reference in [SHA, "main"] {
        let content_hits = 1;
        let server = github_file_server(content_hits).await;
        pages_at(&server, reference).await;
        drop(server);
    }
}

async fn pages_at(server: &MockServer, reference: &str) {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = runtime
        .run(
            "gh-page-1".into(),
            "ghGetFileContent".into(),
            json!({
                "queries":[{"owner":"a","repo":"b","path":"src/lib.rs","ref":reference,
                    "refresh":true}],
                "responseChars": 800
            }),
        )
        .await
        .expect("first page");
    let next = support::checks::leads(&first.structured_content)
        .into_iter()
        .find(|call| call.kind == "continue" && call.tool == "ghGetFileContent")
        .expect("source evidence continuation")
        .query;
    support::assert_cursor_only(&next);
    let mut captured = None;

    for page in ["gh-page-2", "gh-page-2-again"] {
        let replayed = runtime
            .run(page.into(), "ghGetFileContent".into(), next.clone())
            .await
            .expect("replayed page");
        let content = replayed.structured_content["results"][0]["data"]["content"].clone();
        assert!(
            content.as_str().is_some_and(|text| text.contains("remote")),
            "{}",
            replayed.structured_content
        );
        if let Some(previous) = &captured {
            assert_eq!(&content, previous);
        } else {
            captured = Some(content);
        }
    }

    let mut stale = next;
    stale["cursor"] = json!(format!("cursor2.{}", "A".repeat(44)));
    let error = runtime
        .run("gh-page-stale".into(), "ghGetFileContent".into(), stale)
        .await
        .expect_err("a mismatched snapshot never executes");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    runtime.close().await;
}

#[tokio::test]
async fn saved_github_pages_check_current_home_credentials_without_network_replay() {
    use octocode_native::providers::github::{CredentialStore, StoredCredentials};
    for tool in ["ghGetFileContent", "clasify"] {
        let server = github_file_server(1).await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(|request: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&request.body).expect("provider request JSON");
                let probabilities: serde_json::Map<_, _> = (0..64)
                    .map(|index| (format!("label{index:02}"), json!(usize::from(index == 0))))
                    .collect();
                let answers: serde_json::Map<_, _> = body["questions"].as_object().expect("provider questions object").keys()
                    .map(|key| (key.clone(), json!({"type":"choice","choice":"label00","confidence":1,"probabilities":probabilities})))
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({"model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
            })
            .expect(u64::from(tool == "clasify"))
            .mount(&server).await;
        let workspace = Workspace::new();
        let store = CredentialStore::new(&workspace.home);
        let save = |token: &str| {
            let credentials: StoredCredentials = serde_json::from_value(json!({
                "hostname":"127.0.0.1","username":"fixture","token":{"token":token,"tokenType":"oauth"},
                "gitProtocol":"https","createdAt":"","updatedAt":""
            })).expect("stored credential fixture");
            store.save(&credentials).expect("save home credential");
        };
        save("first-local-token");
        let env = [
            ("OCTOCODE_STORAGE_MODE", "persistent".into()),
            ("GITHUB_TOKEN", "".into()),
            ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
            ("OCTOCODE_BETA", "true".into()),
            ("OCTOCODE_CLASSIFICATION_API", "fixture".into()),
            ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ];
        let runtime = workspace.runtime(&env);
        let source = json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA,
            "refresh":true});
        let row = if tool == "clasify" {
            let criteria: serde_json::Map<_, _> = (0..64)
                .map(|index| (format!("label{index:02}"), json!("criterion")))
                .collect();
            let questions: serde_json::Map<_, _> = (0..12).map(|index| (format!("q{index}"),
                json!({"type":"choice","instructions":"Choose a label.","criteria":criteria}))).collect();
            json!({"operation":"evaluate","source":{"tool":"ghGetFileContent","query":{"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA}},"questions":questions})
        } else {
            source
        };
        let first = runtime
            .run(
                "auth-first".into(),
                tool.into(),
                json!({"queries":[row],"responseChars":800}),
            )
            .await
            .expect("first");
        let next = support::checks::leads(&first.structured_content)
            .into_iter()
            .find(|call| call.kind == "continue" && call.tool == tool)
            .unwrap_or_else(|| panic!("continuation: {}", first.structured_content))
            .query;
        support::assert_cursor_only(&next);
        octocode_native::contracts::validate(tool, next.clone()).expect("public snapshot contract");
        let before_replay = server
            .received_requests()
            .await
            .expect("initial requests")
            .len();
        runtime.close().await;
        let runtime = workspace.runtime(&env);
        let same = runtime
            .run("auth-same".into(), tool.into(), next.clone())
            .await
            .expect("same local identity");
        support::checks::assert_output_contract(tool, &same.structured_content);
        save("switched-local-token");
        let changed = runtime
            .run("auth-switched".into(), tool.into(), next.clone())
            .await
            .expect_err("local identity changed");
        assert_eq!(changed.code, ErrorCode::ResponseSnapshotExpired);
        runtime.close().await;
        let runtime = workspace.runtime(&env);
        let changed = runtime
            .run("auth-switched-process".into(), tool.into(), next)
            .await
            .expect_err("new process identity changed");
        assert_eq!(changed.code, ErrorCode::ResponseSnapshotExpired);
        runtime.close().await;
        let requests = server
            .received_requests()
            .await
            .expect("recorded provider requests");
        assert_eq!(
            requests.len(),
            before_replay,
            "saved reads must not call either provider"
        );
    }
}

/// A saved ghGetHistoryItem page answers its `responsePagination.next`
/// after a persistent runtime restart (a second CLI process): every page to
/// the last replays the stored result, and no page re-reads GitHub.
#[tokio::test]
async fn saved_history_item_pages_replay_after_a_persistent_restart() {
    let server = MockServer::builder().start().await;
    let files = (0..60)
        .map(|n| json!({"filename":format!("src/module_{n:03}/file.rs"),"status":"modified","additions":n,"deletions":1}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA,
            "commit": {"message": "many files", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "files": files
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let env = [
        ("OCTOCODE_STORAGE_MODE", "persistent".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
    ];
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run(
            "history-page-1".into(),
            "ghGetHistoryItem".into(),
            json!({
                "queries":[{"operation":"commit","owner":"a","repo":"b","ref":SHA,"sections":["files"]}],
                "responseChars":1000
            }),
        )
        .await
        .expect("first page");
    let mut pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{}", first.structured_content);
    runtime.close().await;
    let total_pages = pagination["totalPages"].clone();
    let mut files = filenames(&first.structured_content);
    let mut page = 1;
    while pagination["hasMore"] == true {
        let next = pagination["next"]["input"].clone();
        support::assert_cursor_only(&next);
        let runtime = workspace.runtime(&env);
        let replayed = runtime
            .run(
                format!("history-page-{}", page + 1),
                "ghGetHistoryItem".into(),
                next,
            )
            .await
            .unwrap_or_else(|error| panic!("page {} after a restart: {error:?}", page + 1));
        runtime.close().await;
        page += 1;
        pagination = replayed.structured_content["responsePagination"].clone();
        files.extend(filenames(&replayed.structured_content));
        assert_eq!(pagination["page"], page, "{pagination}");
        assert_eq!(pagination["totalPages"], total_pages, "{pagination}");
        assert_eq!(pagination["replayed"], true, "{pagination}");
        assert_ne!(pagination["restart"], true, "{pagination}");
    }
    assert!(page > 1, "{pagination}");
    files.sort();
    let expected = (0..60)
        .map(|n| format!("src/module_{n:03}/file.rs"))
        .collect::<Vec<_>>();
    assert_eq!(files, expected, "every changed file exactly once");
}

/// The changed-file paths a history page's rows carry (each file is a
/// `"<status> +<additions> -<deletions> <path>"` line).
fn filenames(page: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for row in page["results"].as_array().into_iter().flatten() {
        for file in row["data"]["files"].as_array().into_iter().flatten() {
            let line = file.as_str().expect("a changed-file line");
            let path = line.rsplit(' ').next().expect("a path");
            found.push(path.to_owned());
        }
    }
    found
}
