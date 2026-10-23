//! Provider failures: malformed patterns, rate limits, recovery-hint wording and provider diagnostics.

use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_github::mock_http::{Arrivals, Delayed};
use serde_json::json;
use std::time::Duration;
use support::{ALL_EXTRAS, Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn malformed_remote_regex_keeps_original_error_and_required_file_path() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("example source\n"),"size":15,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for extras in [json!([]), json!(["diagnostics"])] {
        let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":"(","regex":"rust","extras":extras})).await.expect("error row");
        assert_eq!(row_status(&out), "error", "{}", out.structured_content);
        let data = row_data(&out);
        assert_ne!(data["errorCode"], "outputContractViolation", "{data}");
        assert_eq!(data["path"], "README.md", "{data}");
        assert!(
            data["error"]
                .as_str()
                .is_some_and(|error| error.contains("Invalid regex pattern")),
            "{data}"
        );
    }
    // A list with one unclosed call: the error names the item, and the one
    // lead is the same GitHub read with that item escaped; no static text.
    let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":["^example","source("],"regex":"rust"})).await.expect("error row");
    let data = row_data(&out);
    assert_eq!(data["errorCode"], "invalidPattern", "{data}");
    assert!(
        data["error"].to_string().contains("matchString[1]"),
        "{data}"
    );
    let repair = &crate::support::checks::hint(data, "repair");
    assert_eq!(repair["tool"], "ghGetFileContent", "{data}");
    let row = &repair["query"]["queries"][0];
    assert_eq!(
        row["matchString"],
        json!(["^example", "source\\("]),
        "{data}"
    );
    assert_eq!(
        (
            row["owner"].as_str(),
            row["repo"].as_str(),
            row["ref"].as_str()
        ),
        (Some("a"), Some("b"), Some(sha)),
        "{data}"
    );
    assert!(
        crate::support::checks::hint(data, "text").is_null(),
        "{data}"
    );
    runtime.close().await;
}

/// A primary rate limit surfaces as a contract-valid error row carrying
/// rate-limit metadata (incl. resource), and the blocking fact is mirrored to
/// `<home>/tmp/ratelimit/` for other processes.
#[tokio::test]
async fn gh_primary_rate_limit_is_contract_valid_and_persisted_for_other_processes() {
    let server = MockServer::builder().start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_STORAGE_MODE", "persistent".into()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for name in ["one.rs", "two.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name, "ref": sha, "forceRefresh": true}),
        )
        .await
        .expect("row-level error");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{}",
            outcome.structured_content
        );
        checks::assert_output_contract("ghGetFileContent", &outcome.structured_content);
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "rateLimited", "{data}");
        assert_eq!(data["rateLimit"]["resetEpochSeconds"], reset, "{data}");
        assert_eq!(data["rateLimit"]["resource"], "core", "{data}");
        // An authenticated caller is not told to authenticate.
        checks::assert_absent(data, "auth login");
    }
    // The second read was refused before sending (mock expects one hit).
    let dir = workspace.home.join("tmp").join("ratelimit");
    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("ratelimit dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files[0].path()).unwrap()).unwrap();
    assert_eq!(state["buckets"]["core"]["remaining"], 0, "{state}");
    assert_eq!(state["buckets"]["core"]["reset"], reset, "{state}");
    runtime.close().await;
}

/// Only an anonymous caller is told that authenticating raises quota.
#[tokio::test]
async fn anonymous_rate_limit_advises_authentication() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "4102444800")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("GITHUB_TOKEN", String::new()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner": "a", "repo": "b", "path": "x.rs", "ref": sha, "forceRefresh": true}),
    )
    .await
    .expect("row-level error");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "rateLimited", "{data}");
    assert!(
        crate::support::checks::hint(data, "text")[0]
            .as_str()
            .is_some_and(|hint| hint.contains("auth login")),
        "{data}"
    );
    runtime.close().await;
}

/// Every hint string anywhere in a row.
fn all_hints(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "hints"
                    && let Some(hints) = child.as_array()
                {
                    out.extend(hints.iter().filter_map(|h| h.as_str().map(str::to_owned)));
                }
                out.extend(all_hints(child));
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| out.extend(all_hints(v))),
        _ => {}
    }
    out
}

/// Authentication and binary-file hints arrive whole, never cut
/// mid-sentence with an ellipsis.
#[tokio::test]
async fn github_recovery_hints_are_never_cut_mid_sentence() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/locked"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/img.png"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64",
            "content": STANDARD.encode([0x89u8, b'P', b'N', b'G', 0, 0, 1]),
            "size": 7, "sha": "f".repeat(40), "path": "img.png"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for query in [
        json!({"owner":"a","repo":"b","path":"README.md","ref":"locked","extras":[]}),
        json!({"owner":"a","repo":"b","path":"img.png","ref":sha,"extras":[]}),
    ] {
        let outcome = call(&runtime, "ghGetFileContent", query)
            .await
            .expect("error row");
        let data = row_data(&outcome);
        let hints = all_hints(data);
        assert!(!hints.is_empty(), "{data}");
        for hint in hints {
            assert!(!hint.contains('…'), "truncated hint {hint:?} in {data}");
        }
    }
    runtime.close().await;
}

/// A failed read's provider status, retry flag, request id and
/// documentation link are support diagnostics (`diagnostics` extra): a
/// default row drops them; `extras: ["diagnostics"]` keeps them.
#[tokio::test]
async fn gh_file_error_keeps_provider_diagnostics_for_the_diagnostics_extra() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fmissing.rs"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-github-request-id", "R1")
                .set_body_json(json!({
                    "message":"Not Found",
                    "documentation_url":"https://docs.github.com/rest/repos/contents#get-repository-content"
                })),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for debug in [false, true] {
        let extras = if debug {
            json!(["diagnostics"])
        } else {
            json!([])
        };
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"src/missing.rs","ref":sha,"extras":extras}),
        )
        .await
        .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{data}");
        assert_eq!(data.get("httpStatus").is_some(), debug, "{data}");
        // A 404 is not retryable, so no mode states `retryable`.
        assert!(data.get("retryable").is_none(), "{data}");
        assert_eq!(data.get("requestId").is_some(), debug, "{data}");
        assert_eq!(data.get("documentationUrl").is_some(), debug, "{data}");
    }
    runtime.close().await;
}
