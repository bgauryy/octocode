//! `octocode auth`: the text and JSON answers, the exit class of each
//! failure, and what login reports. Logout
//! deletes from the OS credential store, so its tests live beside the code with
//! an injected store.
use crate::support::Workspace;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::process::{Command, Output};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap_or("")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap_or("")
}

fn json_of(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is JSON ({error}): {}", stdout(output)))
}

/// The CLI with a private temp dir: a login that succeeds stops the warm
/// servers under the temp dir, and a test must never reach a real one.
fn cli(workspace: &Workspace) -> Command {
    let mut command = workspace.cli();
    command.env("TMPDIR", &workspace.home);
    command
}

async fn run(mut command: Command) -> Output {
    tokio::task::spawn_blocking(move || command.output().expect("run the CLI"))
        .await
        .expect("join")
}

async fn github_user(login: Option<&str>) -> MockServer {
    let server = MockServer::builder().start().await;
    let response = match login {
        Some(login) => ResponseTemplate::new(200).set_body_json(json!({"login": login})),
        None => ResponseTemplate::new(401)
            .set_body_json(json_fixture(r#"{"message": "Bad credentials"}"#)),
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

/// A stored OAuth login for `127.0.0.1`, the host of `UNMOCKED_GITHUB_API_URL`.
#[cfg(unix)]
fn store_credentials(workspace: &Workspace) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(
        workspace.home.join("credentials.json"),
        include_str!("../../runtime/tests/fixtures/auth/main-credentials.enc"),
    )
    .expect("credentials");
    std::fs::write(
        workspace.home.join(".key"),
        include_str!("../../runtime/tests/fixtures/auth/main-key.hex"),
    )
    .expect("key");
    for name in ["credentials.json", ".key"] {
        std::fs::set_permissions(
            workspace.home.join(name),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("permissions");
    }
}

#[tokio::test]
async fn status_names_the_account_github_verified() {
    let server = github_user(Some("fixture-user")).await;
    let workspace = Workspace::new();
    let mut command = cli(&workspace);
    command
        .env("GITHUB_TOKEN", "fixture-pat")
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .args(["auth", "status"]);
    let output = run(command).await;
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output).trim(),
        "authenticated as fixture-user (source: env)"
    );
}

#[test]
fn status_says_when_github_could_not_be_asked() {
    let workspace = Workspace::new();
    let output = cli(&workspace)
        .env("GITHUB_TOKEN", "fixture-pat")
        .args(["auth", "status"])
        .output()
        .expect("auth status");
    // The fixture API is a closed port: still authenticated, but unverified.
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output).trim(),
        "authenticated (source: env; unverified: GitHub could not be reached)"
    );
}

#[tokio::test]
async fn status_of_a_rejected_token_names_the_fix() {
    let server = github_user(None).await;
    let workspace = Workspace::new();
    let mut command = cli(&workspace);
    command
        .env("GITHUB_TOKEN", "revoked-fixture-token")
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .args(["auth", "status"]);
    let output = run(command).await;
    assert_eq!(output.status.code(), Some(4));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let text = stderr(&output);
    // The facts of the rejection: the token's source, GitHub's status, and
    // the login command that replaces it.
    for fact in ["env", "401", "octocode auth login"] {
        assert!(text.contains(fact), "{fact}: {text}");
    }
}

#[test]
fn status_without_any_token_is_unauthenticated_with_the_login_step() {
    let workspace = Workspace::new();
    let text = cli(&workspace)
        .args(["auth", "status"])
        .output()
        .expect("auth status");
    assert_eq!(text.status.code(), Some(4));
    let lines = stderr(&text).lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "unauthenticated");
    // The login step names the login command and every token variable.
    for fact in std::iter::once("octocode auth login").chain(
        octocode_runtime_base::config::ENV_TOKEN_VARS
            .iter()
            .copied(),
    ) {
        assert!(lines[1].contains(fact), "{fact}: {}", lines[1]);
    }

    let json = cli(&workspace)
        .args(["auth", "status", "--json"])
        .output()
        .expect("auth status json");
    assert_eq!(json.status.code(), Some(4));
    let value = json_of(&json);
    assert_eq!(value["authenticated"], false, "{value}");
    assert_eq!(value["verification"], "none", "{value}");
    assert_eq!(value["tokenSource"], "none", "{value}");
    assert_eq!(value["username"], Value::Null, "{value}");
    assert_eq!(value["hostname"], "127.0.0.1", "{value}");
}

#[test]
fn login_treats_a_blank_hostname_as_the_configured_host() {
    let workspace = Workspace::new();
    // The fixture API host is an enterprise host without a client id; a blank
    // `--hostname` must not turn into the public github.com flow.
    for blank in ["", "   "] {
        let output = cli(&workspace)
            .args(["auth", "login", "--hostname", blank, "--json"])
            .output()
            .expect("auth login");
        assert_eq!(output.status.code(), Some(4), "{blank:?}");
        let value = json_of(&output);
        assert_eq!(value["errorCode"], "missingConfiguration", "{value}");
        assert!(
            value["error"]
                .as_str()
                .is_some_and(|message| message.contains("OCTOCODE_GITHUB_CLIENT_ID")),
            "{value}"
        );
    }
    let text = cli(&workspace)
        .args(["auth", "login"])
        .output()
        .expect("auth login");
    assert_eq!(text.status.code(), Some(4));
    assert!(stderr(&text).contains("OCTOCODE_GITHUB_CLIENT_ID is required"));
}

#[test]
fn login_refresh_without_a_stored_credential_is_an_authentication_failure() {
    let workspace = Workspace::new();
    let text = cli(&workspace)
        .args(["auth", "login", "--hostname", "github.com", "--refresh"])
        .output()
        .expect("auth login --refresh");
    assert_eq!(text.status.code(), Some(4), "{}", stderr(&text));
    // The next step names the login command and every token variable.
    let next_step = stderr(&text);
    for fact in std::iter::once("octocode auth login").chain(
        octocode_runtime_base::config::ENV_TOKEN_VARS
            .iter()
            .copied(),
    ) {
        assert!(next_step.contains(fact), "{fact}: {next_step}");
    }

    let json = cli(&workspace)
        .args([
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--refresh",
            "--json",
        ])
        .output()
        .expect("auth login --refresh --json");
    assert_eq!(json.status.code(), Some(4));
    let value = json_of(&json);
    assert_eq!(value["errorCode"], "authentication", "{value}");
    assert!(value["error"].is_string(), "{value}");
}

/// A live stored login is kept; only `--force` starts a new one.
#[cfg(unix)]
#[test]
fn login_keeps_a_live_stored_credential_unless_forced() {
    let workspace = Workspace::new();
    store_credentials(&workspace);
    let mut kept = cli(&workspace);
    kept.env("OCTOCODE_GITHUB_CLIENT_ID", "fixture-client")
        .args(["auth", "login"]);
    let text = kept.output().expect("auth login");
    assert_eq!(text.status.code(), Some(0), "{}", stderr(&text));
    assert_eq!(
        stdout(&text).trim(),
        "Already authenticated as legacy-home-user on 127.0.0.1. Use `--force` to switch accounts."
    );

    let mut json = cli(&workspace);
    json.env("OCTOCODE_GITHUB_CLIENT_ID", "fixture-client")
        .args(["auth", "login", "--json"]);
    let value = json_of(&json.output().expect("auth login --json"));
    assert_eq!(value["success"], true, "{value}");
    assert_eq!(value["action"], "none", "{value}");
    assert_eq!(value["alreadyAuthenticated"], true, "{value}");
    assert_eq!(value["hostname"], "127.0.0.1", "{value}");
    assert_eq!(value["username"], "legacy-home-user", "{value}");

    // `--force` goes on to the device flow, which needs a terminal. The
    // environment token that would outrank the new login is named first.
    let mut forced = cli(&workspace);
    forced
        .env("OCTOCODE_GITHUB_CLIENT_ID", "fixture-client")
        .env("GITHUB_TOKEN", "fixture-pat")
        .args(["auth", "login", "--force"]);
    let forced = forced.output().expect("auth login --force");
    assert_eq!(forced.status.code(), Some(2), "{}", stderr(&forced));
    let text = stderr(&forced);
    // The overriding variable is named before the terminal refusal.
    let named = text
        .find("GITHUB_TOKEN")
        .unwrap_or_else(|| panic!("{text}"));
    let refused = text.find("terminal").unwrap_or_else(|| panic!("{text}"));
    assert!(named < refused, "{text}");
}
