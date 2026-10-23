use super::*;

#[test]
fn clasify_missing_key_is_actionable() {
    let workspace = Workspace::new();
    let query = serde_json::json!({"queries":[{
        "id":"decision",
        "mainGoal": "test", "reasoning":"Choose the next inspection.",
        "state":{"fact":"present"},
        "questions":{"relevant":{"type":"noul","instructions":"Is it relevant?"}}
    }]});
    let output = workspace
        .cli()
        .env("OCTOCODE_BETA", "true")
        .args(["clasify", &query.to_string(), "--json"])
        .output()
        .expect("missing-key execution");
    assert_eq!(exit_code(&output), Some(5));
    let output = stdout(&output);
    assert!(output.contains("missingConfiguration"), "{output}");
    assert!(output.contains("OCTOCODE_CLASSIFICATION_API"), "{output}");
    assert!(
        output.contains("https://docs.typesafe.ai/introduction"),
        "{output}"
    );

    let beta_off = workspace
        .cli()
        .env("OCTOCODE_CLASSIFICATION_API", "fixture-key")
        .args(["clasify", &query.to_string(), "--json"])
        .output()
        .expect("missing-beta execution");
    assert_eq!(exit_code(&beta_off), Some(5));
    let beta_off = stdout(&beta_off);
    assert!(beta_off.contains("missingConfiguration"), "{beta_off}");
    assert!(beta_off.contains("OCTOCODE_BETA"), "{beta_off}");
}

#[test]
fn clasify_with_every_resource_failed_exits_by_failure_class() {
    let workspace = Workspace::new();
    let run = |question: serde_json::Value| {
        let query = serde_json::json!({"queries":[{
            "mainGoal": "test", "reasoning":"Exercise failure exit codes.",
            "state":{"fact":"present"},
            "questions":{"q":question}
        }]});
        workspace
            .cli()
            .env("OCTOCODE_BETA", "true")
            .env("OCTOCODE_CLASSIFICATION_API", "dummy")
            .env("OCTOCODE_CLASSIFICATION_API_HOST", "http://127.0.0.1:9")
            .args(["clasify", &query.to_string()])
            .output()
            .expect("clasify execution")
    };
    // A provider that cannot be reached is an execution failure, not bad input.
    let unreachable = run(serde_json::json!({"type":"noul","instructions":"Is it relevant?"}));
    assert_eq!(exit_code(&unreachable), Some(5), "{}", stdout(&unreachable));
    // A removed question type rejects the caller's request.
    let removed = run(serde_json::json!({"type":"locate","instructions":"fact"}));
    assert_eq!(exit_code(&removed), Some(2), "{}", stdout(&removed));
}

#[test]
fn blank_classification_key_disables_clasify_despite_home_and_vendor_keys() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        "OCTOCODE_CLASSIFICATION_API=from-home-env\n",
    )
    .unwrap();
    let query = serde_json::json!({"queries":[{
        "id":"decision",
        "mainGoal": "test", "reasoning":"Exercise the opt-out.",
        "state":{"fact":"present"},
        "questions":{"relevant":{"type":"noul","instructions":"Is it relevant?"}}
    }]})
    .to_string();
    let output = workspace
        .cli()
        .env("OCTOCODE_CLASSIFICATION_API", "")
        // The retired vendor alias is not a credential.
        .env("OCTOCODE_JEV_KEY", "vendor-key")
        .args(["clasify", &query, "--json"])
        .output()
        .unwrap();
    assert_eq!(exit_code(&output), Some(5), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("missingConfiguration"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn auth_login_requires_supported_credentials_and_a_terminal() {
    let workspace = Workspace::new();
    let login = workspace
        .cli()
        .args(["auth", "login", "--hostname", "github.com"])
        .output()
        .expect("auth login");
    // No terminal for the device flow: the caller's request (2).
    assert_eq!(login.status.code(), Some(2));
    // The fixture's API host is an enterprise host without a client id.
    let enterprise = workspace
        .cli()
        .args(["auth", "login"])
        .output()
        .expect("auth login");
    assert_eq!(enterprise.status.code(), Some(5));
}

#[test]
fn auth_status_reports_an_unreachable_env_token_as_unverified() {
    let workspace = Workspace::new();
    // `auth` requires its subcommand: `status` is not implied.
    let bare = workspace.cli().arg("auth").output().expect("bare auth");
    assert_eq!(exit_code(&bare), Some(2), "{}", stdout(&bare));
    // Unreachable API: the token cannot be verified, so it stays
    // authenticated but says so.
    let output = workspace
        .cli()
        .env("GITHUB_TOKEN", "fixture-pat")
        .env("GITHUB_API_URL", "http://127.0.0.1:1/api/v3")
        .args(["auth", "status", "--json"])
        .output()
        .expect("auth status");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("auth json");
    assert_eq!(value["authenticated"], true, "{value}");
    assert_eq!(value["verification"], "unverified", "{value}");
    assert_eq!(value["tokenSource"], "env", "{value}");
}

/// A whitespace-only env token must NOT be reported as an `env` credential:
/// the request path resolves tokens through `resolve_env_token`, which trims
/// and drops empty values, and `auth status` now shares that same selection
/// (`config.token`). Before the flows were unified the diagnostic re-read the
/// raw var and reported `env` for a token the request path would never send.
#[test]
fn auth_status_ignores_whitespace_only_env_token() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("GH_TOKEN", "   ")
        .args(["auth", "status", "--json"])
        .output()
        .expect("auth status");
    assert!(output.status.success() || output.status.code() == Some(4));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("auth json");
    assert_ne!(
        value["tokenSource"], "env",
        "whitespace-only token must not resolve as an env credential: {value}"
    );
}

#[test]
fn config_shows_files_and_keys_but_never_values() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["config", "--json"])
        .output()
        .expect("config json");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("config JSON");
    assert!(value["configFile"]["path"].is_string());
    assert!(value["configFile"]["exists"].is_boolean());
    for scope in ["global", "project"] {
        assert!(value["envFiles"][scope]["path"].is_string(), "{value}");
        assert!(value["envFiles"][scope]["exists"].is_boolean(), "{value}");
    }
    assert!(value["envKeys"].is_array());

    let human = workspace.cli().arg("config").output().expect("config");
    assert!(human.status.success(), "{}", stderr(&human));
    let text = stdout(&human);
    assert!(text.contains("\nconfig\n  global "), "{text}");
    assert!(text.contains("\n.env\n  global "), "{text}");
    assert!(
        text.contains("Values are not shown here; `octocode config get KEY` prints one."),
        "missing no-values note: {text}"
    );
}

#[test]
fn config_check_reports_set_state_without_the_value() {
    let workspace = Workspace::new();
    let set = workspace
        .cli()
        .env("GITHUB_TOKEN", "fixture-secret")
        .args(["config", "check", "GITHUB_TOKEN"])
        .output()
        .expect("config check");
    assert!(set.status.success(), "{}", stderr(&set));
    assert!(
        stdout(&set).contains("GITHUB_TOKEN: set"),
        "{}",
        stdout(&set)
    );
    assert!(
        !stdout(&set).contains("fixture-secret"),
        "value leaked: {}",
        stdout(&set)
    );
    let unset = workspace
        .cli()
        .args(["config", "check", "OCTOCODE_NOT_A_KEY"])
        .output()
        .expect("config check unset");
    assert_eq!(exit_code(&unset), Some(1));
    assert!(stdout(&unset).contains("unset"), "{}", stdout(&unset));
}
