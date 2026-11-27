//! `longContext` on a `sections` row means what it means on `evaluate`: omitted
//! asks for the long window only after the default one refuses, `false` refuses
//! unbilled and never asks for it.

use super::{Typed, clasify, noul};
use crate::support::{Workspace, clasify_runtime};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

const LONG: &str = "typesafe-danger-zone-long-context";

/// Refuses every request in the default window and answers it in the long one.
async fn long_window_only() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            if request.headers.get(LONG).is_none() {
                return ResponseTemplate::new(400).set_body_json(json_fixture(
                    r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
                ));
            }
            Typed.respond(request)
        })
        .mount(&server)
        .await;
    server
}

/// Requests received so far, and how many of them asked for the long window.
async fn posts(server: &MockServer) -> (usize, usize) {
    let requests = server.received_requests().await.unwrap();
    let long = requests
        .iter()
        .filter(|request| request.headers.get(LONG).is_some())
        .count();
    (requests.len(), long)
}

fn sections_row(long_context: Option<bool>) -> Value {
    let mut row = json!({"operation":"sections",
        "resources":{"r":{"tool":"localFetch","query":{"path":"notes.txt"}}},
        "questions":{"q":noul("Does this section mention a restart?")}});
    if let Some(long_context) = long_context {
        row["longContext"] = json!(long_context);
    }
    row
}

fn retry_notice(out: &Value) -> bool {
    out["results"][0]["data"]["warnings"]
        .as_array()
        .is_some_and(|hints| {
            hints
                .iter()
                .filter_map(Value::as_str)
                .any(|hint| hint.contains("refused"))
        })
}

#[tokio::test]
async fn a_sections_row_with_long_context_false_refuses_unbilled_and_never_retries() {
    let server = long_window_only().await;
    let workspace = Workspace::new();
    workspace.write("notes.txt", "Restart the service after the update.\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);

    let out = clasify(&runtime, vec![sections_row(Some(false))]).await;
    assert_eq!(
        posts(&server).await,
        (1, 0),
        "false: only the unbilled default-window request: {out}"
    );
    assert!(!retry_notice(&out), "no long-context retry notice: {out}");
    assert!(
        !out.to_string().contains("\"status\":\"answered\""),
        "the refused cell is not answered: {out}"
    );

    // The omitted twin still retries in the long window once refused.
    let out = clasify(&runtime, vec![sections_row(None)]).await;
    assert_eq!(
        posts(&server).await,
        (3, 1),
        "omitted: refused, then the long window: {out}"
    );
    assert!(retry_notice(&out), "the retry is disclosed: {out}");
    runtime.close().await;
}
