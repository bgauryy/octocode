use super::*;

/// A failed login answers with the one `{error, errorCode}` envelope and
/// exits by the code's class: an enterprise host without a client id is
/// missing configuration (5); a login without a terminal is the caller's
/// request (2).
#[test]
fn json_auth_failures_use_the_error_envelope_and_class() {
    let workspace = Workspace::new();
    for (args, code, exit) in [
        (
            vec![
                "auth",
                "login",
                "--hostname",
                "audit.example.invalid",
                "--json",
            ],
            "missingConfiguration",
            5,
        ),
        (
            vec![
                "auth",
                "login",
                "--hostname",
                "github.com",
                "--force",
                "--json",
            ],
            "invalidInput",
            2,
        ),
    ] {
        let output = workspace
            .cli()
            .args(&args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{args:?}");
        let envelope = serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
        assert_eq!(envelope["errorCode"], code, "{envelope}");
        assert!(envelope["error"].is_string(), "{envelope}");
    }
}

fn manage(
    workspace: &Workspace,
    mut request: serde_json::Value,
) -> (Option<i32>, serde_json::Value) {
    use std::io::Write;
    use std::process::Stdio;
    if request.get("expectedFingerprint").is_none() {
        request["expectedFingerprint"] = workspace.runtime(&[]).catalog()["fingerprint"].clone();
    }
    let mut child = workspace
        .cli()
        .args(["config", "--manage"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("request");
    let output = child.wait_with_output().expect("output");
    (
        output.status.code(),
        serde_json::from_slice(&output.stdout).expect("management response"),
    )
}

#[test]
fn managed_edits_type_a_missing_revision_as_invalid_input_and_a_stale_one_as_conflict() {
    let workspace = Workspace::new();
    let edit = serde_json::json!({"operation": "setEnv", "scope": "home", "key": "TEST_KEY", "value": "v"});
    let (code, missing) = manage(&workspace, edit.clone());
    assert_eq!(code, Some(2));
    assert_eq!(missing["error"]["code"], "INVALID_INPUT", "{missing}");
    let mut stale = edit;
    stale["revision"] = "stale".into();
    let (code, stale) = manage(&workspace, stale);
    assert_eq!(code, Some(5), "a conflict is retryable after a refresh");
    assert_eq!(stale["error"]["code"], "CONFLICT", "{stale}");
    assert!(!workspace.home.join(".env").exists());
}

/// A view built against another contract cannot be served: the build is
/// unavailable to it, not its input wrong.
#[test]
fn management_contract_drift_is_unavailable() {
    let workspace = Workspace::new();
    let (code, drift) = manage(
        &workspace,
        serde_json::json!({"operation": "inspect", "expectedFingerprint": "not-this-build"}),
    );
    assert_eq!(code, Some(5));
    assert_eq!(drift["success"], false, "{drift}");
    assert_eq!(drift["error"]["code"], "UNAVAILABLE", "{drift}");
}

#[test]
fn unknown_settings_are_rejected_without_mutating_saved_configuration() {
    let workspace = Workspace::new();
    let path = workspace.home.join(".octocoderc");
    let original = r#"{"lsp":{"prewarm":"off"}}"#;
    std::fs::write(&path, original).unwrap();
    let (_, inspected) = manage(&workspace, serde_json::json!({"operation": "inspect"}));
    let revision = inspected["data"]["files"]["homeSettings"]["revision"].clone();
    let (code, rejected) = manage(
        &workspace,
        serde_json::json!({"operation":"setSetting","scope":"home","key":"unrecognized.option",
            "value":"private-marker","revision":revision}),
    );
    assert_eq!(code, Some(2));
    assert_eq!(rejected["error"]["code"], "INVALID_INPUT", "{rejected}");
    assert!(!rejected.to_string().contains("private-marker"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn arbitrary_environment_keys_remain_editable_and_values_are_not_disclosed() {
    let workspace = Workspace::new();
    let path = workspace.home.join(".env");
    std::fs::write(&path, "KEEP=original\n").unwrap();
    let set = workspace
        .cli()
        .args([
            "config",
            "set",
            "CUSTOM_PROVIDER_KEY",
            "private-marker",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stdout)
    );
    assert!(!String::from_utf8_lossy(&set.stdout).contains("private-marker"));
    let (_, inspected) = manage(&workspace, serde_json::json!({"operation":"inspect"}));
    assert!(!inspected.to_string().contains("private-marker"));
    assert!(
        inspected["data"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key["key"] == "CUSTOM_PROVIDER_KEY" && key["set"] == true)
    );
    let unset = workspace
        .cli()
        .args(["config", "unset", "CUSTOM_PROVIDER_KEY", "--json"])
        .output()
        .unwrap();
    assert!(unset.status.success());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "KEEP=original\n");
}

#[test]
fn unknown_saved_settings_warn_without_appearing_as_resolved_settings() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".octocoderc"),
        r#"{"unrecognized":{"key":"private-marker"},"lsp":{"prewarm":"off"}}"#,
    )
    .unwrap();
    let show = workspace.cli().args(["config", "--json"]).output().unwrap();
    assert!(show.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert!(!shown.to_string().contains("private-marker"));
    let diagnostics = shown["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{shown}");
    assert_eq!(diagnostics[0]["code"], "unknown_or_future_config");
    let (_, inspected) = manage(&workspace, serde_json::json!({"operation":"inspect"}));
    let settings = inspected["data"]["settings"].as_array().unwrap();
    assert!(
        settings
            .iter()
            .any(|setting| setting["key"] == "lsp.prewarm" && setting["value"] == "off")
    );
    assert!(
        !settings
            .iter()
            .any(|setting| setting["key"].as_str().unwrap().starts_with("unrecognized"))
    );
    assert!(!inspected.to_string().contains("private-marker"));
}

#[test]
fn login_normalizes_the_public_github_host_before_requiring_a_client_id() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["auth", "login", "--hostname", "GitHub.com", "--json"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("login");
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json");
    let error = body["error"].as_str().expect("error");
    assert!(!error.contains("OCTOCODE_GITHUB_CLIENT_ID"), "{error}");
    assert!(error.contains("interactive terminal"), "{error}");
}
