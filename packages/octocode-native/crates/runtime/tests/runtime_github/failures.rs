//! Provider failures: malformed patterns, rate limits, recovery-hint wording and provider diagnostics.

use crate::support::{self};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::json_fixture;
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn malformed_remote_regex_keeps_the_original_error() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "README.md").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("example source\n"),"size":15,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for patterns in [
        json_fixture(r#"["(?:x"]"#),
        json_fixture(r#"["^example", "[z-a]"]"#),
    ] {
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
    // A bracket that pairs with nothing reads as text, and the call says so;
    // a miss then names no lead the row lacks (the support checks hold it).
    for (patterns, status, read) in [
        (
            json!(["^example", "source("]),
            "success",
            "matchString[1] as `source\\(`",
        ),
        (json!(["("]), "empty", "matchString[0] as `\\(`"),
    ] {
        let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":patterns,"regexEngine":"rust"})).await.expect("runs");
        assert_eq!(row_status(&out), status, "{}", out.structured_content);
        let said = out.structured_content["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|warning| warning.as_str())
            .any(|warning| warning.starts_with("read ") && warning.contains(read));
        assert!(said, "{}", out.structured_content);
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
                .set_body_json(json_fixture(r#"{"message": "API rate limit exceeded"}"#)),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // No caller budget: a spent core quota is disclosed at once, not
    // waited out for up to an hour.
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_STORAGE_MODE", "persistent".into()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for name in ["one.rs", "two.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name, "ref": sha}),
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
    // The second read was refused before sending. The first call's core
    // reads (the contents read, and the commit-list date read after GraphQL
    // fails) may both go out before either is answered; nothing of the
    // second file reaches GitHub.
    let core_reads: Vec<String> = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.method.as_str() == "GET")
        .map(|request| request.url.to_string())
        .collect();
    assert!(
        (1..=2).contains(&core_reads.len()) && core_reads.iter().all(|url| !url.contains("two.rs")),
        "{core_reads:?}"
    );
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
                .set_body_json(json_fixture(r#"{"message": "API rate limit exceeded"}"#)),
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
        json!({"owner": "a", "repo": "b", "path": "x.rs", "ref": sha}),
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
            ResponseTemplate::new(401)
                .set_body_json(json_fixture(r#"{"message": "Bad credentials"}"#)),
        )
        .mount(&server)
        .await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "img.png").await;
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
            json_fixture(
                r#"{
                    "owner": "a",
                    "repo": "b",
                    "path": "README.md",
                    "ref": "locked"
                }"#,
            ),
            "authentication",
            "auth login",
        ),
        (
            json!({"owner":"a","repo":"b","path":"img.png","ref":sha}),
            "binaryFileUnsupported",
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

/// A failure names what an agent acts on, not GitHub's support diagnostics.
#[tokio::test]
async fn gh_file_error_omits_provider_support_diagnostics() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fmissing.rs"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-github-request-id", "R1")
                .set_body_json(json_fixture(r#"{
                    "message": "Not Found",
                    "documentation_url": "https://docs.github.com/rest/repos/contents#get-repository-content"
                }"#)),
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
    assert!(
        data.get("httpStatus").is_none(),
        "notFound already names the 404: {data}"
    );
    assert!(
        data.get("retryable").is_none(),
        "404 is not retryable: {data}"
    );
    assert!(data["error"].is_string(), "{data}");
    runtime.close().await;
}
