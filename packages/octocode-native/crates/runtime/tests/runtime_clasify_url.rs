//! clasify `source: {url}` through the real runtime: every refusal that
//! must happen before any network work or provider call. Extraction,
//! fragments, redirects, size limits and the capture's read lead are unit
//! tests over an injected fetcher (`source_capture::url`).

use crate::runtime_clasify::{clasify, sent};
use crate::support::{Run as _, Workspace, capture_allowed, clasify_runtime};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use wiremock::MockServer;

fn url_row(url: &str) -> Value {
    json!({
        "operation": "evaluate",
        "source": {"url": url},
        "questions": {"q": {"type":"noul","instructions":"Does the page say to restart?"}}
    })
}

#[tokio::test]
async fn memory_storage_refuses_a_url_source_before_any_work() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[
            ("OCTOCODE_STORAGE_MODE", "memory".into()),
            capture_allowed(&workspace),
        ],
    );
    let out = clasify(&runtime, vec![url_row("https://127.0.0.1:9/guide")]).await;
    let error = &out["results"][0]["data"];
    assert_eq!(
        error["errorCode"], "classificationContextUnavailable",
        "{out}"
    );
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("persistent storage"),
        "{out}"
    );
    assert!(out["results"][0]["data"].get("answers").is_none());
    assert_eq!(sent(&server).await.len(), 0);
    assert!(
        !workspace.home.join("tmp/capture").exists(),
        "memory storage writes no capture"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_non_https_url_is_rejected_at_validation() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[capture_allowed(&workspace)]);
    let error = runtime
        .run(
            "clasify-http".into(),
            "clasify".into(),
            json!({"queries": [url_row("http://docs.example.org/guide")]}),
        )
        .await
        .expect_err("the contract admits only https URLs");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert_eq!(sent(&server).await.len(), 0);
    runtime.close().await;
}

#[tokio::test]
async fn a_url_source_needs_local_fetch_to_read_its_capture_back() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[
            ("DISABLE_TOOLS", "localFetch".into()),
            capture_allowed(&workspace),
        ],
    );
    let error = runtime
        .run_mcp(
            "clasify-no-reader".into(),
            "clasify".into(),
            json!({"queries": [url_row("https://docs.example.org/guide")]}),
        )
        .await
        .expect("MCP refusal response");
    assert_eq!(error["structuredContent"]["errorCode"], "invalidInput");
    assert!(
        error["structuredContent"]["error"]
            .as_str()
            .is_some_and(|message| message.contains("queries.0.source")
                && message.contains("localFetch")
                && message.contains("DISABLE_TOOLS")),
        "{error}"
    );
    assert_eq!(sent(&server).await.len(), 0);
    runtime.close().await;
}

#[tokio::test]
async fn a_url_with_credentials_is_refused_before_any_work() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[capture_allowed(&workspace)]);
    // Input security refuses it at validation; the capture's own userinfo
    // check (unit-tested) also covers redirects.
    let error = runtime
        .run(
            "clasify-creds".into(),
            "clasify".into(),
            json!({"queries": [url_row("https://user:hunter2@docs.example.org/guide")]}),
        )
        .await
        .expect_err("a credential in the URL is refused");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert!(!format!("{error:?}").contains("hunter2"), "{error:?}");
    assert_eq!(sent(&server).await.len(), 0);
    runtime.close().await;
}

#[tokio::test]
async fn internal_addresses_are_refused_per_row() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[capture_allowed(&workspace)]);
    // Nothing listens on port 9: a connection attempt would surface as
    // classificationContextFailed, never as a security refusal.
    let out = clasify(
        &runtime,
        vec![
            url_row("https://127.0.0.1:9/guide"),
            url_row("https://localhost:9/guide"),
            url_row("https://169.254.169.254/latest/meta-data/"),
            url_row("https://[fd00::1]:9/guide"),
        ],
    )
    .await;
    let rows = out["results"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "{out}");
    for row in rows {
        assert_eq!(
            row["data"]["errorCode"], "securityValidationFailed",
            "{}: {out}",
            row["index"]
        );
        assert!(row["data"].get("answers").is_none());
    }
    assert_eq!(sent(&server).await.len(), 0);
    let saved = std::fs::read_dir(workspace.home.join("tmp/capture/url"))
        .map(Iterator::count)
        .unwrap_or(0);
    assert_eq!(saved, 0, "a refused URL saves no capture");
    runtime.close().await;
}
