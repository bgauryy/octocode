//! Provider failures: malformed patterns, rate limits, recovery-hint wording and provider diagnostics.

use crate::support::{self};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn malformed_remote_regex_keeps_the_original_error() {
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
    for patterns in [json!(["("]), json!(["^example", "source("])] {
        let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":patterns,"regexEngine":"rust"})).await.expect("error row");
        assert_eq!(row_status(&out), "error");
        let data = row_data(&out);
        assert_eq!(data["errorCode"], "invalidPattern", "{data}");
        assert!(data.get("path").is_none(), "{data}");
        assert!(
            data["error"]
                .as_str()
                .is_some_and(|message| message.contains("regex") || message.contains("pattern")),
            "{data}"
        );
        assert!(
            checks::leads(data).is_empty(),
            "invalid patterns have no guessed repair: {data}"
        );
    }
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
            json!({"owner": "a", "repo": "b", "path": name, "ref": sha, "refresh": true}),
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
        json!({"owner": "a", "repo": "b", "path": "x.rs", "ref": sha, "refresh": true}),
    )
    .await
    .expect("row-level error");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "rateLimited", "{data}");
    assert!(
        data["hints"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|hint| hint
                .as_str()
                .is_some_and(|text| text.contains("auth login"))),
        "{data}"
    );
    runtime.close().await;
}

/// Errors without saved work preserve their reason and complete recovery guidance.
#[tokio::test]
async fn github_errors_without_continuations_preserve_reason_and_recovery() {
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
    for (query, code, guidance) in [
        (
            json!({"owner":"a","repo":"b","path":"README.md","ref":"locked"}),
            "authentication",
            "auth login",
        ),
        (
            json!({"owner":"a","repo":"b","path":"img.png","ref":sha}),
            "invalidInput",
            "Binary content",
        ),
    ] {
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let outcome = call(&runtime, "ghGetFileContent", query.clone())
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"));
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], code, "{data}");
        assert!(data.get("next").is_none(), "{data}");
        assert!(
            outcome
                .structured_content
                .get("responsePagination")
                .is_none(),
            "{}",
            outcome.structured_content
        );
        let hints = data["hints"].as_array().expect("recovery hints");
        assert!(
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|text| text.contains(guidance))),
            "{data}"
        );
        for hint in hints {
            assert!(
                !hint.as_str().expect("text").contains('…'),
                "truncated hint {hint:?} in {data}"
            );
        }
        runtime.close().await;
    }
}

/// Provider diagnostics survive the same stable error shape on every call.
#[tokio::test]
async fn gh_file_error_preserves_provider_diagnostics() {
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
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"src/missing.rs","ref":sha}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert_eq!(data["httpStatus"], 404, "{data}");
    assert!(
        data.get("retryable").is_none(),
        "404 is not retryable: {data}"
    );
    assert_eq!(data["requestId"], "R1", "{data}");
    assert!(
        data["documentationUrl"]
            .as_str()
            .is_some_and(|url| url.contains("get-repository-content")),
        "{data}"
    );
    runtime.close().await;
}
