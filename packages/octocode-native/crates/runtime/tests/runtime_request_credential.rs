//! A host-supplied per-request GitHub token (`admit_with({githubToken})`, the
//! napi `execute*` 4th argument): the request's only credential, isolated
//! per token, never echoed, never replaced by an ambient credential.
#![allow(clippy::unwrap_used)]
use crate::support::{Workspace, envelope, row_data, row_status};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_native::runtime::{RuntimeError, ToolOutcome, ToolRuntime};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn read_as(runtime: &ToolRuntime, token: &str, file: &str) -> ToolOutcome {
    let admission = runtime
        .admit_with("req".into(), Some(json!({"githubToken": token})))
        .expect("admitted");
    runtime
        .execute_admitted(
            admission,
            "ghGetFileContent".into(),
            envelope(json!({"owner":"a","repo":"b","path":file,"ref":SHA})),
        )
        .await
        .expect("row result")
}

async fn sends_by(server: &MockServer, token: &str) -> usize {
    let bearer = format!("Bearer {token}");
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| {
            request
                .headers
                .get("authorization")
                .is_some_and(|value| value == bearer.as_str())
        })
        .count()
}

fn file(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "type":"file","encoding":"base64","content":STANDARD.encode(body),
        "size":body.len(),"sha":"a".repeat(40),"path":"secret.txt"
    }))
}

/// Two callers of one runtime get separate cache and rate-limit partitions:
/// B never sees A's cached private file, and A's spent quota never blocks B.
#[tokio::test]
async fn request_tokens_get_isolated_cache_and_rate_partitions() {
    let server = MockServer::builder().start().await;
    let (alice, bob) = ("request-token-alice", "request-token-bob");
    let (carol, dave) = ("request-token-carol", "request-token-dave");
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/secret.txt"))
        .and(header("authorization", format!("Bearer {alice}").as_str()))
        .respond_with(file("alice private body\n"))
        .mount(&server)
        .await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    Mock::given(method("GET"))
        .and(header("authorization", format!("Bearer {carol}").as_str()))
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
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/open.txt"))
        .and(header("authorization", format!("Bearer {dave}").as_str()))
        .respond_with(file("dave body\n"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .with_priority(10)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    let first = read_as(&runtime, alice, "secret.txt").await;
    assert_eq!(
        row_status(&first),
        "success",
        "{}",
        first.structured_content
    );
    assert_eq!(row_data(&first)["content"], "1\talice private body\n");
    let other = read_as(&runtime, bob, "secret.txt").await;
    assert_eq!(row_status(&other), "error", "{}", other.structured_content);
    assert!(
        !other
            .structured_content
            .to_string()
            .contains("alice private")
    );
    assert!(
        sends_by(&server, bob).await >= 1,
        "bob's read reached GitHub"
    );

    for _ in 0..2 {
        let spent = read_as(&runtime, carol, "open.txt").await;
        assert_eq!(
            row_data(&spent)["errorCode"],
            "rateLimited",
            "{}",
            spent.structured_content
        );
    }
    let fresh = read_as(&runtime, dave, "open.txt").await;
    assert_eq!(
        row_status(&fresh),
        "success",
        "{}",
        fresh.structured_content
    );
    assert_eq!(
        sends_by(&server, &workspace.token).await,
        0,
        "the ambient token is never sent"
    );
    runtime.close().await;
}

/// GitHub rejects a supplied token: the row is an auth error with a hint for
/// a remote caller, the token is never echoed, and nothing falls back to the
/// env, stored, or `gh` credential. The rejected token stays the request's
/// credential on the next call (it is not added to the shared rejected set).
#[cfg(unix)]
#[tokio::test]
async fn rejected_request_token_never_falls_back_or_echoes() {
    let server = MockServer::builder().start().await;
    let bad = format!("ghp_{}", "Z".repeat(36));
    Mock::given(method("GET"))
        .and(header("authorization", format!("Bearer {bad}").as_str()))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"message": format!("Bad credentials {bad}")})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(file("ambient body\n"))
        .with_priority(10)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut config = workspace.config(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("PATH", workspace.fake_gh_path()),
    ]);
    config.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let runtime = ToolRuntime::new(config).unwrap();

    for _ in 0..2 {
        let admission = runtime
            .admit_with("req".into(), Some(json!({"githubToken": bad})))
            .unwrap();
        let result = runtime
            .execute_mcp_admitted(
                admission,
                "ghGetFileContent".into(),
                envelope(json!({"owner":"a","repo":"b","path":"x.txt","ref":SHA})),
            )
            .await
            .unwrap();
        let text = result.to_string();
        assert!(!text.contains(&bad), "token echoed: {text}");
        assert!(!text.contains("ambient body"), "fell back: {text}");
        assert!(!text.contains("auth login"), "local-only hint: {text}");
        let data = &result["structuredContent"]["results"][0]["data"];
        assert_eq!(data["errorCode"], "authentication", "{text}");
        assert!(text.contains("supplied with this request"), "{text}");
    }
    assert_eq!(
        sends_by(&server, &bad).await,
        2,
        "the supplied token, every time"
    );
    assert_eq!(
        sends_by(&server, &workspace.token).await,
        0,
        "no env fallback"
    );
    assert!(!workspace.home.join("gh-calls").exists(), "no gh fallback");
    runtime.close().await;
}

/// Request options are strict and never echoed: an unknown key or a blank
/// token is refused before any work (a typo must not mean ambient fallback).
#[tokio::test]
async fn request_options_are_strict_and_never_echoed() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let secret = format!("ghp_{}", "Q".repeat(36));
    for options in [
        json!({"githubTok": secret}),
        json!({"githubToken": "  "}),
        json!({"githubToken": 5}),
    ] {
        let error: RuntimeError = runtime
            .admit_with("req".into(), Some(options))
            .err()
            .expect("refused");
        assert_eq!(error.code, "invalidInput");
        assert!(!error.message.contains(&secret), "{}", error.message);
    }
    assert!(runtime.admit_with("req".into(), Some(Value::Null)).is_ok());
    runtime.close().await;
}
