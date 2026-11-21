//! A row held to a `maxRequests` that a default-window refusal would spend:
//! the cap leaves no request for the long-window retry the refusal leads to,
//! so nothing is sent and the row never dead-ends. It states one price before
//! any spend (its text names `requestsCeiling`, past the cap) and
//! `next.confirm` runs it within that price.

use super::{Typed, clasify, noul};
use crate::support::{Workspace, clasify_runtime};
use octocode_test_support::json_fixture;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

const LONG: &str = "typesafe-danger-zone-long-context";
/// The largest body the stand-in provider's default window takes.
const DEFAULT_WINDOW_BYTES: usize = 100_000;

/// Refuses a body over [`DEFAULT_WINDOW_BYTES`] in the default window;
/// answers every request the long window is asked for.
async fn default_window_refuses() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            if request.headers.get(LONG).is_none() && request.body.len() > DEFAULT_WINDOW_BYTES {
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

#[tokio::test]
async fn a_spent_cap_offers_a_priced_confirm_instead_of_a_dead_end() {
    let server = default_window_refuses().await;
    let workspace = Workspace::new();
    // Over the default window, inside the stated long one.
    let text = (1..=1_800)
        .map(|line| format!("Entry {line}: the nightly job copied {line} records and exited.\n"))
        .collect::<String>();
    assert!(text.len() > DEFAULT_WINDOW_BYTES && text.len() < crate::support::LONG_WINDOW_TOKENS);
    workspace.write("big.txt", text);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let cap = 1_u64;
    let row = json!({"operation":"evaluate","maxRequests":cap,
        "source":{"tool":"localFetch","query":{"path":"big.txt"}},
        "questions":{"copied":noul("Does the job copy records?")}});

    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    let sent = server.received_requests().await.unwrap();
    assert!(
        sent.is_empty(),
        "a request whose refusal the cap cannot follow is priced, not sent: {out}"
    );
    assert_eq!(data["errorCode"], "classificationOverBudget", "{out}");
    assert_eq!(data["usage"]["calls"], json!(sent.len()), "{out}");
    let ceiling = data["usage"]["requestsCeiling"]
        .as_u64()
        .expect("a stated ceiling");
    assert!(ceiling > cap, "the price passes the spent cap: {out}");
    let error = data["error"].as_str().expect("error");
    assert!(
        error.contains(&format!("at most {ceiling} ")),
        "the text states the usage's ceiling: {out}"
    );
    let confirm = &data["next"]["confirm"];
    assert_eq!(confirm["tool"], "clasify", "{out}");
    assert!(confirm["input"].is_object(), "an executable confirm: {out}");

    let before = sent.len();
    let confirmed = crate::support::Run::run(
        &runtime,
        "confirm".into(),
        "clasify".into(),
        confirm["input"].clone(),
    )
    .await
    .expect("the confirm call runs")
    .structured_content
    .into_value();
    let data = &confirmed["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{confirmed}");
    assert!(data["answers"]["copied"].is_object(), "{confirmed}");
    let spent = server.received_requests().await.unwrap().len() - before;
    assert!(
        spent as u64 <= ceiling,
        "{spent} past {ceiling}: {confirmed}"
    );
    assert_eq!(data["usage"]["calls"], json!(spent), "{confirmed}");
    runtime.close().await;
}
