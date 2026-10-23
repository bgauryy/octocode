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

/// Internal limits are not settings: `config set` and the config view's
/// `setEnv`/`setSetting` reject a removed name with the reason and write
/// nothing, while `config unset` still deletes a stale line.
#[test]
fn config_set_rejects_a_removed_setting() {
    let workspace = Workspace::new();
    let path = workspace.home.join(".env");
    std::fs::write(&path, "KEEP=original\n").unwrap();
    for key in [
        "REQUEST_TIMEOUT",
        "OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH",
        "network.timeout",
        "output.pagination.defaultCharLength",
    ] {
        let output = workspace
            .cli()
            .args(["config", "set", key, "5000", "--json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{key}");
        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["errorCode"], "invalidInput", "{key}: {body}");
        assert!(
            body["error"].as_str().unwrap().contains("removed"),
            "{key}: {body}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "KEEP=original\n");
    }
    let (_, inspected) = manage(&workspace, serde_json::json!({"operation": "inspect"}));
    for (operation, file, key) in [
        ("setEnv", "homeEnv", "REQUEST_TIMEOUT"),
        ("setSetting", "homeSettings", "network.timeout"),
    ] {
        let revision = inspected["data"]["files"][file]["revision"].clone();
        let (code, rejected) = manage(
            &workspace,
            serde_json::json!({"operation": operation, "scope": "home", "key": key,
                "value": if operation == "setEnv" { serde_json::json!("5000") } else { serde_json::json!(5000) },
                "revision": revision}),
        );
        assert_eq!(code, Some(2), "{operation}: {rejected}");
        assert_eq!(rejected["error"]["code"], "INVALID_INPUT", "{rejected}");
        assert!(
            rejected.to_string().contains("removed"),
            "{operation}: {rejected}"
        );
    }
    assert!(!workspace.home.join(".octocoderc").exists());
    std::fs::write(&path, "KEEP=original\nREQUEST_TIMEOUT=\"9000\"\n").unwrap();
    let unset = workspace
        .cli()
        .args(["config", "unset", "REQUEST_TIMEOUT", "--json"])
        .output()
        .unwrap();
    assert!(
        unset.status.success(),
        "{}",
        String::from_utf8_lossy(&unset.stdout)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "KEEP=original\n");
}

/// A removed limit is not listed as a setting or a loaded key: `config
/// --json` reports it once as `removed_setting`, `config get` reads it as
/// unset, and the config view's settings no longer include it.
#[test]
fn config_views_no_longer_list_removed_limits() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        "REQUEST_TIMEOUT=9000\nOCTOCODE_LSP_PREWARM=off\n",
    )
    .unwrap();
    std::fs::write(
        workspace.home.join(".octocoderc"),
        r#"{"network":{"timeout":9000},"storage":{"cloneCache":{"maxClones":3}}}"#,
    )
    .unwrap();
    let show = workspace.cli().args(["config", "--json"]).output().unwrap();
    assert!(show.status.success());
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let env_keys = shown["envKeys"].as_array().unwrap();
    assert!(
        env_keys.iter().any(|key| key == "OCTOCODE_LSP_PREWARM"),
        "{shown}"
    );
    assert!(
        !env_keys.iter().any(|key| key == "REQUEST_TIMEOUT"),
        "{shown}"
    );
    let removed: Vec<&serde_json::Value> = shown["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "removed_setting")
        .collect();
    assert_eq!(removed.len(), 3, "{shown}");
    for key in ["REQUEST_TIMEOUT", "network.timeout"] {
        let get = workspace
            .cli()
            .args(["config", "get", key, "--json"])
            .output()
            .unwrap();
        assert_eq!(get.status.code(), Some(1), "{key}");
        let got: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
        assert_eq!(got["set"], false, "{key}: {got}");
    }
    let (_, inspected) = manage(&workspace, serde_json::json!({"operation": "inspect"}));
    let settings: Vec<&str> = inspected["data"]["settings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|setting| setting["key"].as_str())
        .collect();
    assert!(settings.contains(&"lsp.prewarm"), "{settings:?}");
    for removed in [
        "network.timeout",
        "network.maxRetries",
        "network.maxRateLimitWait",
        "lsp.maxMemoryMb",
        "output.pagination.defaultCharLength",
        "storage.cloneCache.ttl",
        "classification.maxConcurrency",
        "classification.timeoutMs",
    ] {
        assert!(!settings.contains(&removed), "{removed}: {settings:?}");
    }
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
