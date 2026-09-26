#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(unix)]
mod support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use support::{Workspace, call, row_status};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

fn fake_gh(workspace: &Workspace) -> String {
    let script = workspace.write("bin/gh", "#!/bin/sh\n[ \"$1 $2 $3 $4\" = 'auth token --hostname 127.0.0.1' ] || exit 2\n[ -z \"$GH_TOKEN$GITHUB_TOKEN$OCTOCODE_TOKEN\" ] || exit 3\nprintf x >> \"$OCTOCODE_HOME/gh-calls\"\nprintf synthetic-gh-credential\n");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    format!("{}:/usr/bin:/bin", script.parent().unwrap().display())
}
#[tokio::test]
async fn native_cli_reads_main_home_credentials_before_gh() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join("credentials.json"),
        include_str!("fixtures/auth/main-credentials.enc"),
    )
    .unwrap();
    std::fs::write(
        workspace.home.join(".key"),
        include_str!("fixtures/auth/main-key.hex"),
    )
    .unwrap();
    for name in ["credentials.json", ".key"] {
        std::fs::set_permissions(
            workspace.home.join(name),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    let mut command = workspace.cli();
    command
        .env("PATH", fake_gh(&workspace))
        .env("GITHUB_API_URL", "http://127.0.0.1:1/api/v3")
        .args(["auth", "status", "--json"]);
    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["username"], "legacy-home-user");
    assert_eq!(value["tokenSource"], "octocode-storage");
    assert!(!workspace.home.join("gh-calls").exists());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-"));
}
#[tokio::test]
async fn native_cli_status_uses_host_scoped_gh_and_preserves_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", "Bearer synthetic-gh-credential"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"fixture-user"})))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command
        .env("PATH", fake_gh(&workspace))
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .args(["auth", "status", "--json"]);
    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tokenSource"], "gh-cli");
    assert_eq!(value["username"], "fixture-user");
    assert_eq!(value["authenticated"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-gh-credential"));
    assert_eq!(
        std::fs::read_to_string(workspace.home.join("gh-calls")).unwrap(),
        "x"
    );
}
#[tokio::test]
async fn mcp_runtime_discovers_once_and_pins_credential_across_github_requests() {
    let server = MockServer::start().await;
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
    let mut config = workspace.config(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("PATH", fake_gh(&workspace)),
    ]);
    config.env.remove("OCTOCODE_TOKEN");
    config.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let runtime = octocode_native::runtime::ToolRuntime::new(config).unwrap();
    let result = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"readme.txt","branch":"main","forceRefresh":true}),
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
    let mut config = workspace.config(&[("PATH", fake_gh(&workspace))]);
    config.env.remove("OCTOCODE_TOKEN");
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
