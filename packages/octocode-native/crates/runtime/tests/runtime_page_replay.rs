#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use support::Workspace;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn lines(word: &str) -> String {
    (0..200)
        .map(|index| format!("needle {word} {index}\n"))
        .collect()
}

#[tokio::test]
async fn a_matching_snapshot_pages_the_stored_envelope_without_re_executing() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", lines("original"));
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let first = runtime
        .execute(
            "page-1".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":root,"searchText":"needle","pageSize":50,
                    "goal":"Find needles.","reasoning":"Exercise page replay."}],
                "responseCharLength": 800
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let snapshot = pagination["snapshot"].clone();
    let next = pagination["next"]["query"].clone();
    assert_eq!(next["responseSnapshot"], snapshot, "{pagination}");

    // A re-execution would now see different rows, change the snapshot and
    // demand a restart; the stored envelope keeps serving the same response.
    workspace.write("src/a.txt", lines("rewritten"));
    let second = runtime
        .execute("page-2".into(), "localSearch".into(), next.clone())
        .await
        .expect("second page");
    let page = &second.structured_content["responsePagination"];
    assert_eq!(page["snapshot"], snapshot, "{page}");
    assert_ne!(page["restart"], true, "{page}");
    assert!(page["charOffset"].as_u64().unwrap_or(0) > 0, "{page}");
    let text = second
        .content
        .iter()
        .map(|content| content.text.as_str())
        .collect::<String>();
    assert!(!text.contains("rewritten"), "page 2 re-executed: {text}");

    let mut stale: Value = next;
    stale["responseSnapshot"] = json!(format!("{}0", snapshot.as_str().unwrap()));
    let third = runtime
        .execute("page-stale".into(), "localSearch".into(), stale)
        .await
        .expect("stale page");
    let page = &third.structured_content["responsePagination"];
    assert_eq!(page["restart"], true, "{page}");
    assert_ne!(
        page["snapshot"], snapshot,
        "a mismatched snapshot re-executes"
    );
    runtime.close().await;
}

/// Dispatch counter: the content endpoint is hit once for the first page and
/// once more only for the page whose snapshot does not match the stored one.
async fn github_file_server(content_hits: u64) -> MockServer {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
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

#[tokio::test]
async fn a_matching_snapshot_does_not_re_dispatch_and_a_mismatch_does() {
    let server = github_file_server(2).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = runtime
        .execute(
            "gh-page-1".into(),
            "ghGetFileContent".into(),
            json!({
                "queries":[{"owner":"a","repo":"b","path":"src/lib.rs","branch":"main",
                    "forceRefresh":true,"fullContent":true,
                    "goal":"Read the file.","reasoning":"Exercise page replay."}],
                "responseCharLength": 800
            }),
        )
        .await
        .expect("first page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let next = pagination["next"]["query"].clone();
    assert_eq!(next["queries"][0]["forceRefresh"], true, "{next}");

    for page in ["gh-page-2", "gh-page-2-again"] {
        let replayed = runtime
            .execute(page.into(), "ghGetFileContent".into(), next.clone())
            .await
            .expect("replayed page");
        let page = &replayed.structured_content["responsePagination"];
        assert_eq!(page["snapshot"], pagination["snapshot"], "{page}");
        assert_ne!(page["restart"], true, "{page}");
    }

    let mut stale = next;
    stale["responseSnapshot"] = json!("response-rows-v2:stale");
    runtime
        .execute("gh-page-stale".into(), "ghGetFileContent".into(), stale)
        .await
        .expect("a mismatched snapshot re-executes");
    runtime.close().await;
    drop(server);
}

#[tokio::test]
async fn rows_scope_pages_replay_the_stored_envelope() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", lines("original"));
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let row = |word: &str| json!({"path":root,"searchText":word,"goal":"Find needles.","reasoning":"Exercise page replay."});
    let first = runtime
        .execute(
            "rows-1".into(),
            "localSearch".into(),
            json!({"queries":[row("needle"), row("original")],
                "responseScope":"rows","responseCharLength": 1200}),
        )
        .await
        .expect("first rows page");
    let pagination = first.structured_content["responsePagination"].clone();
    assert_eq!(pagination["hasMore"], true, "{pagination}");
    let snapshot = pagination["snapshot"].as_str().unwrap().to_owned();
    assert!(snapshot.starts_with("response-rows-v2"), "{snapshot}");
    let next = pagination["next"]["query"].clone();
    assert_eq!(next["responseSnapshot"], snapshot.as_str(), "{pagination}");

    workspace.write("src/a.txt", lines("rewritten"));
    let second = runtime
        .execute("rows-2".into(), "localSearch".into(), next)
        .await
        .expect("second rows page");
    let page = &second.structured_content["responsePagination"];
    assert_eq!(page["snapshot"], snapshot.as_str(), "{page}");
    assert_ne!(page["restart"], true, "{page}");
    assert!(
        !second.structured_content.to_string().contains("rewritten"),
        "rows page 2 re-executed"
    );
    runtime.close().await;
}
