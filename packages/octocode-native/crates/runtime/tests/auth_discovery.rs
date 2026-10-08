#![allow(clippy::expect_used, clippy::unwrap_used)]
#![cfg(unix)]
use crate::support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use support::{Workspace, call, row_status};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn mcp_runtime_discovers_once_and_pins_credential_across_github_requests() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .and(header("authorization", "Bearer synthetic-gh-credential"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/readme.txt"))
        .and(header("authorization", "Bearer synthetic-gh-credential"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),"size":6,"sha":"a".repeat(40),"path":"readme.txt"}))).expect(1).mount(&server).await;
    let workspace = Workspace::new();
    let gh_path = workspace.fake_gh_path();
    // macOS can delay the first launch of a newly written executable. Warm only
    // the synthetic helper outside the runtime's auth deadline, then reset its
    // marker so the assertions below still count runtime discovery alone.
    let warmup = tokio::process::Command::new(workspace.workspace.join("bin/gh"))
        .args(["auth", "token", "--hostname", "127.0.0.1"])
        .env_clear()
        .env("OCTOCODE_HOME", &workspace.home)
        .kill_on_drop(true)
        .output();
    let warmup = tokio::time::timeout(std::time::Duration::from_secs(60), warmup)
        .await
        .expect("synthetic helper startup deadline")
        .expect("start synthetic helper");
    assert!(warmup.status.success(), "{warmup:?}");
    assert_eq!(warmup.stdout, b"synthetic-gh-credential");
    assert_eq!(
        std::fs::read_to_string(workspace.home.join("gh-calls")).unwrap(),
        "x"
    );
    std::fs::remove_file(workspace.home.join("gh-calls")).unwrap();
    let mut config = workspace.config(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("PATH", gh_path),
    ]);
    config.env.remove("GITHUB_TOKEN");
    config.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let runtime = octocode_native::runtime::ToolRuntime::new(config).unwrap();
    let result = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"readme.txt","ref":"main","forceRefresh":true}),
    )
    .await
    .unwrap();
    assert_eq!(
        row_status(&result),
        "success",
        "{}; gh calls: {:?}; HTTP paths: {:?}",
        result.structured_content,
        std::fs::read_to_string(workspace.home.join("gh-calls")),
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.path())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        std::fs::read_to_string(workspace.home.join("gh-calls")).unwrap(),
        "x"
    );
    runtime.close().await;
}
#[tokio::test]
async fn local_tools_never_discover_github_credentials() {
    let workspace = Workspace::new();
    workspace.write("readme.txt", "hello\n");
    let mut config = workspace.config(&[("PATH", workspace.fake_gh_path())]);
    config.env.remove("GITHUB_TOKEN");
    let runtime = octocode_native::runtime::ToolRuntime::new(config).unwrap();
    let result = call(
        &runtime,
        "localFetch",
        json!({"path":workspace.workspace.join("readme.txt"),"fullContent":true}),
    )
    .await
    .unwrap();
    assert_eq!(row_status(&result), "success");
    assert!(!workspace.home.join("gh-calls").exists());
    runtime.close().await;
}

/// A long-lived runtime whose stored token GitHub starts rejecting (401)
/// reports that one failure, then re-selects and falls back to gh for the
/// next request; the rejected token is sent once, never retried in a loop.
#[tokio::test]
async fn rejected_stored_token_falls_back_to_gh_in_a_running_runtime() {
    use octocode_native::providers::github::{CredentialStore, OAuthToken, StoredCredentials};
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let gh = "Bearer synthetic-gh-credential";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .and(header("authorization", gh))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/readme.txt"))
        .and(header("authorization", gh))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),"size":6,"sha":"a".repeat(40),"path":"readme.txt"}))).mount(&server).await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .with_priority(10)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    CredentialStore::new(&workspace.home)
        .save(&StoredCredentials {
            hostname: "127.0.0.1".into(),
            username: "fixture".into(),
            token: OAuthToken {
                token: "stored-token".into(),
                token_type: "oauth".into(),
                scopes: None,
                refresh_token: None,
                expires_at: None,
                refresh_token_expires_at: None,
            },
            git_protocol: "https".into(),
            created_at: String::new(),
            updated_at: String::new(),
        })
        .unwrap();
    let gh_path = workspace.fake_gh_path();
    // Warm the synthetic helper outside the auth deadline (slow first exec on macOS).
    let warmup = tokio::process::Command::new(workspace.workspace.join("bin/gh"))
        .args(["auth", "token", "--hostname", "127.0.0.1"])
        .env_clear()
        .env("OCTOCODE_HOME", &workspace.home)
        .kill_on_drop(true)
        .output();
    tokio::time::timeout(std::time::Duration::from_secs(60), warmup)
        .await
        .expect("synthetic helper startup deadline")
        .expect("start synthetic helper");
    let mut config = workspace.config(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("PATH", gh_path),
    ]);
    config.env.remove("GITHUB_TOKEN");
    config.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let runtime = octocode_native::runtime::ToolRuntime::new(config).unwrap();
    let query =
        json!({"owner":"a","repo":"b","path":"readme.txt","ref":"main","forceRefresh":true});
    let rejected = call(&runtime, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(
        row_status(&rejected),
        "error",
        "{}",
        rejected.structured_content
    );
    let fallback = call(&runtime, "ghGetFileContent", query).await.unwrap();
    assert_eq!(
        row_status(&fallback),
        "success",
        "{}",
        fallback.structured_content
    );
    let stored_sends = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| {
            request
                .headers
                .get("authorization")
                .is_some_and(|value| value == "Bearer stored-token")
        })
        .count();
    assert_eq!(stored_sends, 1, "the rejected token is sent once");
    runtime.close().await;
}
