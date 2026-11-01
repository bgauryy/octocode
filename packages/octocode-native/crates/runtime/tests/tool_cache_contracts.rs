//! Per-tool cache-contract integration tests.
//!
//! Provider request counts and returned evidence prove cache reuse or refresh.
//! Cache telemetry is absent on every tool's public result rows.

use crate::support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod github_reads;
mod storage;

// ─── helpers ────────────────────────────────────────────────────────────────

fn assert_no_cache_telemetry(outcome: &octocode_native::response::outcome::ToolOutcome) {
    assert!(
        outcome
            .structured_content
            .pointer("/results/0/cache")
            .is_none()
    );
}

/// A 40-char hex string used as a branch SHA that skips commit resolution.
fn sha(ch: char) -> String {
    ch.to_string().repeat(40)
}

// ─── ghSearchHistory ─────────────────────────────────────────────────────────

/// `ghSearchHistory` bypasses `ConditionalCache` intentionally — history
/// search results are mutable. Both calls must reach the provider.
#[tokio::test]
async fn ghsearchhistory_revalidates_without_cache_telemetry() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(2)
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = support::github_api(&server);
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "operation": "pullRequest",
        "owner": "o",
        "repo": "r",
    });
    for _ in 0..2 {
        let outcome = call(&runtime, "ghSearchHistory", query.clone())
            .await
            .unwrap();
        assert_eq!(
            row_status(&outcome),
            "empty",
            "{}",
            outcome.structured_content
        );
        assert_no_cache_telemetry(&outcome);
    }
    runtime.close().await;
}

// ─── artifactSearch ──────────────────────────────────────────────────────────

/// Memory storage reuses registry responses within one runtime, but a new
/// runtime starts cold and neither runtime persists those responses.
#[tokio::test]
async fn artifactsearch_memory_storage_reuses_ram_without_persisting() {
    let server = MockServer::builder().start().await;
    let pkg = "memory-cache-fixture";
    // The npm `exact` code fetches `/{name}/{spec}` where spec defaults to
    // "latest".  The response must be a single-version object (not the full
    // registry envelope) containing at minimum `name` and `version`.
    Mock::given(method("GET"))
        .and(path(format!("/{pkg}/latest")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": pkg,
            "version": "1.0.0",
            "description": "memory cache fixture",
        })))
        .expect(2)
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let registry = server.uri();
    let settings = [
        ("OCTOCODE_STORAGE_MODE", "memory".into()),
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".into()),
    ];
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "operation": "resolve",
        "ecosystem": "npm",
        "packageName": pkg,
        "registryUrl": registry,
    });

    for call_n in 1..=2u32 {
        let outcome = call(&runtime, "artifactSearch", query.clone())
            .await
            .unwrap();
        assert_eq!(
            row_status(&outcome),
            "success",
            "call {call_n} must succeed"
        );
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    runtime.close().await;
    let restarted = workspace.runtime(&settings);
    let outcome = call(&restarted, "artifactSearch", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    assert!(!workspace.home.join("tmp/artifact").exists());
    restarted.close().await;
}

// ─── local tool presentation ────────────────────────────────────────────────

/// Reusing internal work must not add cache telemetry to public evidence.
#[tokio::test]
async fn local_tools_never_set_cache_flag() {
    let workspace = Workspace::new();
    let target = workspace.write(
        "greet.rs",
        b"pub fn greet() -> &'static str { \"hello\" }\n",
    );
    let runtime = workspace.runtime(&[]);

    for _ in 0..2 {
        // localSearch
        let search = call(
            &runtime,
            "localSearch",
            json!({"path": target.parent().unwrap(), "matchString": ["greet"]}),
        )
        .await
        .unwrap();
        assert_no_cache_telemetry(&search);

        // localFetch
        let fetch = call(&runtime, "localFetch", json!({"path": target}))
            .await
            .unwrap();
        assert_no_cache_telemetry(&fetch);
    }
    runtime.close().await;
}
