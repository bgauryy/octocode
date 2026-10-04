#![allow(clippy::expect_used, clippy::unwrap_used)]
#![cfg(unix)]
mod support;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use support::Workspace;
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
        include_str!("../../runtime/tests/fixtures/auth/main-credentials.enc"),
    )
    .unwrap();
    std::fs::write(
        workspace.home.join(".key"),
        include_str!("../../runtime/tests/fixtures/auth/main-key.hex"),
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

/// A token GitHub rejects (401) is present but not authenticated; status
/// never reports it as a working login.
#[tokio::test]
async fn native_cli_status_reports_a_rejected_token_as_invalid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for json_out in [true, false] {
        let mut command = workspace.cli();
        command
            .env("GITHUB_TOKEN", "revoked-fixture-token")
            .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
            .args(["auth", "status"]);
        if json_out {
            command.arg("--json");
        }
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        if json_out {
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["authenticated"], false, "{value}");
            assert_eq!(value["verification"], "invalid", "{value}");
            assert_eq!(value["tokenPresent"], true, "{value}");
        } else {
            assert_eq!(output.status.code(), Some(1));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("invalid token"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
