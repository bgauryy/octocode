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

/// When `storage.mode=memory` the artifact in-process HTTP cache must be
/// bypassed: both calls to the same package URL must reach the HTTP layer.
///
/// Contrast: with the default `persistent` mode the second call is a cache hit
/// (not tested here to avoid process-static cache pollution across tests).
#[tokio::test]
async fn artifactsearch_memory_storage_mode_bypasses_in_process_cache() {
    let server = MockServer::builder().start().await;
    // Use a unique package name so this test doesn't share state with the
    // process-global artifact cache from other tests.
    let pkg = format!("cache-bypass-test-{}", std::process::id());
    // The npm `exact` code fetches `/{name}/{spec}` where spec defaults to
    // "latest".  The response must be a single-version object (not the full
    // registry envelope) containing at minimum `name` and `version`.
    Mock::given(method("GET"))
        .and(path(format!("/{pkg}/latest")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": pkg,
            "version": "1.0.0",
            "description": "cache bypass fixture",
        })))
        .expect(2) // memory mode must not serve the second call from cache
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
    // wiremock enforces expect(2) on drop → fails if second call was cached.
    runtime.close().await;
}

// ─── local tools (no caching) ────────────────────────────────────────────────

/// `localSearch` and `localFetch` never cache their results.  Repeated calls
/// for the same file must not add public cache telemetry.
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
