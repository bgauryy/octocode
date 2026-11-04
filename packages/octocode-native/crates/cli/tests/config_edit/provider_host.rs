//! Provider keys and hosts: the configured host receives the request, a workspace
//! cannot redirect a key, and the settings page offers everything the contract does.
use super::settings::*;
use super::*;
use octocode_native::config::{CONFIG_FIELDS, KNOWN_ENV};
use std::process::Output;

// ─── The host reaches the provider ─────────────────────────────────────

const QUERY: &str = r#"{"queries":[{"operation":"evaluate","content":{"fact":"present"},"questions":{"q":{"type":"noul","instructions":"Is it relevant?"}}}]}"#;

async fn run_clasify(mut command: std::process::Command) -> Output {
    command.args(["clasify", QUERY, "--json"]);
    tokio::task::spawn_blocking(move || command.output().expect("clasify"))
        .await
        .unwrap()
}

/// Key and host set in the home `.octocoderc`, in the home `.env`, and through
/// `config set`: each routes the provider request to the configured host with
/// the configured key.
#[tokio::test]
async fn the_configured_host_receives_the_request_and_the_key_from_every_home_source() {
    for source in ["octocoderc", "dotenv", "config set"] {
        let server = wiremock::MockServer::start().await;
        let workspace = Workspace::new();
        match source {
            "octocoderc" => home_rc(
                &workspace,
                &format!(
                    r#"{{"classification":{{"api":"home-key","apiHost":"{}"}}}}"#,
                    server.uri()
                ),
            ),
            "dotenv" => std::fs::write(
                workspace.home.join(".env"),
                format!(
                    "OCTOCODE_CLASSIFICATION_API=home-key\n{HOST_ENV}={}\n",
                    server.uri()
                ),
            )
            .unwrap(),
            _ => {
                for (key, value) in [
                    ("classification.api", "home-key".to_owned()),
                    (HOST_PATH, server.uri()),
                ] {
                    let set = config(&workspace, &["set", key, &value]);
                    assert_eq!(set.status.code(), Some(0), "{}", errors(&set));
                }
            }
        }
        run_clasify(workspace.cli()).await;
        let requests = server.received_requests().await.unwrap();
        assert!(!requests.is_empty(), "{source}: the host got no request");
        let authorization = requests[0]
            .headers
            .get("authorization")
            .expect("authorization header");
        assert!(
            authorization.to_str().unwrap().contains("home-key"),
            "{source}"
        );
    }
}

/// A host no provider call may use stops the call. It never falls back to the
/// default host, which would send the same key somewhere the user did not choose.
#[tokio::test]
async fn an_unusable_host_fails_closed_instead_of_reaching_another_host() {
    let server = wiremock::MockServer::start().await;
    let address = server.address().to_string();
    for host in [
        format!("http://user:host-secret@{address}"),
        format!("http://{address}/gateway"),
        format!("http://{address}/?token=host-secret"),
    ] {
        let workspace = Workspace::new();
        std::fs::write(
            workspace.home.join(".env"),
            format!("OCTOCODE_CLASSIFICATION_API=home-key\n{HOST_ENV}={host}\n"),
        )
        .unwrap();
        let output = run_clasify(workspace.cli()).await;
        assert_eq!(output.status.code(), Some(5), "{host}: {}", text(&output));
        assert!(
            text(&output).contains("invalidClassificationConfiguration"),
            "{host}: {}",
            text(&output)
        );
        let all = format!("{}{}", text(&output), errors(&output));
        assert!(!all.contains("host-secret"), "{all}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A checked-out repository cannot redirect the user's key: its `.env` and
/// `.octocoderc` hosts are ignored, even when it supplies the key itself.
#[tokio::test]
async fn a_workspace_file_cannot_redirect_the_key_to_its_own_host() {
    let trusted = wiremock::MockServer::start().await;
    let attacker = wiremock::MockServer::start().await;
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        format!(
            "OCTOCODE_CLASSIFICATION_API=home-key\n{HOST_ENV}={}\n",
            trusted.uri()
        ),
    )
    .unwrap();
    let dir = workspace_dir(&workspace);
    std::fs::write(dir.join(".env"), format!("{HOST_ENV}={}\n", attacker.uri())).unwrap();
    std::fs::write(
        dir.join(".octocoderc"),
        format!(r#"{{"classification":{{"apiHost":"{}"}}}}"#, attacker.uri()),
    )
    .unwrap();
    let output = run_clasify(workspace.cli()).await;
    assert!(
        errors(&output).contains("workspace_config_protected"),
        "{}",
        errors(&output)
    );
    assert!(
        attacker.received_requests().await.unwrap().is_empty(),
        "a repository redirected the key"
    );
    assert!(!trusted.received_requests().await.unwrap().is_empty());
}

#[test]
fn a_process_host_overrides_every_file() {
    let workspace = Workspace::new();
    home_rc(
        &workspace,
        r#"{"classification":{"apiHost":"https://home.example"}}"#,
    );
    let output = workspace
        .cli()
        .env(HOST_ENV, "https://process.example")
        .args(["config", "get", HOST_PATH, "--json"])
        .output()
        .unwrap();
    let detail = json_of(&output);
    assert_eq!(detail["value"], "https://process.example", "{detail}");
    assert_eq!(detail["source"], "env", "{detail}");
}

// ─── `config view` management ──────────────────────────────────────────

/// The view edits the host as a setting with its default, not as a hidden
/// credential, and only in home scope.
#[test]
fn the_view_edits_the_host_as_a_home_only_setting() {
    let workspace = Workspace::new();
    let (code, inspected) =
        management::manage(&workspace, serde_json::json!({"operation": "inspect"}));
    assert_eq!(code, Some(0), "{inspected}");
    let inspected = &inspected["data"];
    let settings = inspected["settings"].as_array().expect("settings");
    let host = settings
        .iter()
        .find(|setting| setting["key"] == HOST_PATH)
        .unwrap_or_else(|| panic!("{HOST_PATH} is not an editable setting: {inspected}"));
    assert_eq!(host["credential"], false, "{host}");
    assert_eq!(host["workspaceAllowed"], false, "{host}");
    assert!(
        host["displayDefault"]
            .as_str()
            .is_some_and(|default| default.contains("https://api.typesafe.ai")),
        "{host}"
    );
    let revision = |scope: &str| inspected["files"][scope]["revision"].clone();
    let (code, refused) = management::manage(
        &workspace,
        serde_json::json!({"operation": "setSetting", "scope": "workspace", "key": HOST_PATH,
            "value": "https://proxy.example", "revision": revision("workspaceSettings")}),
    );
    assert_eq!(code, Some(2), "{refused}");
    let (code, refused) = management::manage(
        &workspace,
        serde_json::json!({"operation": "setSetting", "scope": "home", "key": HOST_PATH,
            "value": "https://user:view-secret@proxy.example", "revision": revision("homeSettings")}),
    );
    assert_eq!(code, Some(2), "{refused}");
    assert!(!refused.to_string().contains("view-secret"), "{refused}");
    let (code, saved) = management::manage(
        &workspace,
        serde_json::json!({"operation": "setSetting", "scope": "home", "key": HOST_PATH,
            "value": "https://proxy.example", "revision": revision("homeSettings")}),
    );
    assert_eq!(code, Some(0), "{saved}");
    assert_eq!(
        text(&config(&workspace, &["get", HOST_PATH])),
        "https://proxy.example"
    );
}

/// The page is built from the contract: every setting carries the words and
/// section it needs, and every provider key is offered before it is set.
#[test]
fn the_view_gets_each_settings_description_and_section_and_every_known_key() {
    let workspace = Workspace::new();
    let (code, inspected) =
        management::manage(&workspace, serde_json::json!({"operation": "inspect"}));
    assert_eq!(code, Some(0), "{inspected}");
    let inspected = &inspected["data"];
    let settings = inspected["settings"].as_array().expect("settings");
    for field in CONFIG_FIELDS.iter().filter(|field| !field.credential) {
        let row = settings
            .iter()
            .find(|row| row["key"] == field.path)
            .unwrap_or_else(|| panic!("{} is missing", field.path));
        assert!(
            row["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
            "{row}"
        );
        assert!(
            row["sectionTitle"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
            "{row}"
        );
        assert_eq!(row["env"].as_str(), field.env_name(), "{row}");
    }
    let known = inspected["knownKeys"].as_array().expect("known keys");
    for entry in KNOWN_ENV {
        let row = known
            .iter()
            .find(|row| row["key"] == entry.name)
            .unwrap_or_else(|| panic!("{} is not offered", entry.name));
        assert_eq!(row["credential"], entry.credential, "{row}");
        assert!(
            row["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
            "{row}"
        );
    }
    for key in [
        "OCTOCODE_CLASSIFICATION_API",
        "TAVILY_API_KEY",
        "EXA_API_KEY",
        "SERPER_API_KEY",
        "SCRAPING_ANT",
        "GH_TOKEN",
        "GITHUB_TOKEN",
    ] {
        assert!(known.iter().any(|row| row["key"] == key), "{key}");
    }
    // Endpoints are offered, not secret, and home-only.
    for key in ["TAVILY_BASE_URL", "EXA_BASE_URL", "SERPER_BASE_URL"] {
        let row = known.iter().find(|row| row["key"] == key).expect(key);
        assert_eq!(row["credential"], false, "{row}");
        assert_eq!(row["writableFrom"], "home", "{row}");
    }
    // Env params are on the page too; these only the process may set.
    for key in ["HTTPS_PROXY", "NO_PROXY", "SSL_CERT_FILE", "OCTOCODE_HOME"] {
        let row = known.iter().find(|row| row["key"] == key).expect(key);
        assert_eq!(row["writableFrom"], "process", "{row}");
    }
    assert!(
        !known.iter().any(|row| row["key"] == HOST_ENV),
        "the host is a setting, not a secret"
    );
}

/// Every key and endpoint is visible from one command, secrets masked, and the
/// repository's redirect is reported instead of applied.
#[test]
fn list_and_get_cover_provider_keys_and_endpoints_and_refuse_a_workspace_redirect() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        "TAVILY_API_KEY=home-secret-key\nEXA_BASE_URL=https://home-exa.example\n",
    )
    .unwrap();
    std::fs::write(
        workspace_dir(&workspace).join(".env"),
        "TAVILY_BASE_URL=https://attacker.example\nSERPER_API_KEY=workspace-secret\n",
    )
    .unwrap();
    let listed = json_of(&config(&workspace, &["list", "--json"]));
    assert!(!listed.to_string().contains("secret"), "{listed}");
    let rows = listed["settings"].as_array().expect("settings");
    let row = |key: &str| rows.iter().find(|row| row["key"] == key).unwrap().clone();
    assert_eq!(row("TAVILY_API_KEY")["credential"], true);
    assert_eq!(row("TAVILY_API_KEY")["set"], true);
    assert_eq!(row("TAVILY_API_KEY")["source"], "home");
    assert_eq!(
        row("SERPER_API_KEY")["source"],
        "workspace",
        "a key may come from a workspace"
    );
    assert_eq!(row("EXA_BASE_URL")["value"], "https://home-exa.example");
    assert_eq!(
        row("TAVILY_BASE_URL")["set"],
        false,
        "the repository's host is not applied"
    );
    assert_eq!(row("TAVILY_BASE_URL")["value"], serde_json::Value::Null);
    assert_eq!(row("TAVILY_BASE_URL")["default"], "https://api.tavily.com");
    assert_eq!(row("TAVILY_BASE_URL")["workspace"], "no");
    let shown = json_of(&config(&workspace, &["--json"]));
    assert!(
        shown["skippedProtected"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["key"] == "TAVILY_BASE_URL")),
        "{shown}"
    );

    let unset = config(&workspace, &["get", "TAVILY_BASE_URL"]);
    assert_eq!(unset.status.code(), Some(1));
    assert!(
        errors(&unset).contains("https://api.tavily.com"),
        "{}",
        errors(&unset)
    );
    let masked = config(&workspace, &["get", "TAVILY_API_KEY"]);
    assert!(!text(&masked).contains("secret"), "{}", text(&masked));
    assert!(text(&masked).contains("REDACTED"), "{}", text(&masked));
}

#[test]
fn set_refuses_an_endpoint_that_could_never_work_without_echoing_it() {
    let workspace = Workspace::new();
    for value in ["not-a-url", "https://user:typed-secret@tavily.example"] {
        let output = config(&workspace, &["set", "TAVILY_BASE_URL", value]);
        assert_eq!(output.status.code(), Some(2), "{value}");
        assert!(!format!("{}{}", text(&output), errors(&output)).contains("typed-secret"));
    }
    let ok = config(
        &workspace,
        &["set", "TAVILY_BASE_URL", "https://tavily.example"],
    );
    assert_eq!(ok.status.code(), Some(0), "{}", errors(&ok));
    assert_eq!(
        text(&config(&workspace, &["get", "TAVILY_BASE_URL"])),
        "https://tavily.example"
    );
}

/// A key set in the shell is reported as set, masked: it is in no file the page reads.
#[test]
fn the_view_reports_a_key_set_in_the_shell_without_its_value() {
    use std::io::Write;
    let workspace = Workspace::new();
    let mut child = workspace
        .cli()
        .env("TAVILY_API_KEY", "shell-secret-value")
        .args(["config", "--manage"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    let request = serde_json::json!({
        "operation": "inspect",
        "expectedFingerprint": workspace.runtime(&[]).catalog()["fingerprint"].clone(),
    });
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("request");
    let output = child.wait_with_output().expect("output");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("shell-secret-value"), "{text}");
    let inspected: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let row = inspected["data"]["knownKeys"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["key"] == "TAVILY_API_KEY"))
        .expect("Tavily")
        .clone();
    assert_eq!(row["set"], true, "{row}");
    assert_eq!(row["source"], "env", "{row}");
}
