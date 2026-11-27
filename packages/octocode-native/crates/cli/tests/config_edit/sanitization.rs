use super::{
    Workspace,
    settings::{config, errors, home_rc, text},
};
use serde_json::json;

fn token() -> String {
    format!("ghp_{}", "a".repeat(40))
}

#[test]
fn sensitive_boundary_config_values_and_rejected_keys_are_scrubbed() {
    let workspace = Workspace::new();
    let token = token();
    std::fs::write(
        workspace.home.join(".env"),
        format!("AUDIT_VALUE={token}\n"),
    )
    .unwrap();
    for json in [false, true] {
        for (command, key) in [
            ("get", "AUDIT_VALUE"),
            ("get", token.as_str()),
            ("check", token.as_str()),
        ] {
            let mut args = vec![command, key];
            if json {
                args.push("--json");
            }
            let output = config(&workspace, &args);
            let shown = format!("{}{}", text(&output), errors(&output));
            assert!(!shown.contains(&token), "{command}: {shown}");
            assert!(shown.contains("REDACTED"), "{command}: {shown}");
        }
    }
}

#[test]
fn sensitive_boundary_config_urls_are_scrubbed_and_folder_paths_are_preserved() {
    let workspace = Workspace::new();
    let token = token();
    let folder = workspace
        .workspace
        .join("secrets/credentials/private/source.pem");
    home_rc(
        &workspace,
        &json!({"classification":{"apiHost":format!("https://example.invalid/{token}")},
        "local":{"allowedPaths":[folder]}})
        .to_string(),
    );
    for args in [
        vec!["list"],
        vec!["list", "--json"],
        vec!["get", "classification.apiHost"],
    ] {
        let output = config(&workspace, &args);
        assert!(!text(&output).contains(&token), "{}", text(&output));
    }
    let output = workspace
        .cli()
        .env_remove("ALLOWED_PATHS")
        .args(["config", "get", "local.allowedPaths", "--json"])
        .output()
        .unwrap();
    let shown: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(shown["value"], json!([folder]));
}

#[test]
fn sensitive_boundary_startup_diagnostics_never_echo_rejected_values() {
    let workspace = Workspace::new();
    let token = token();
    home_rc(
        &workspace,
        &json!({"local":{"allowedPaths":[token]}}).to_string(),
    );
    let output = config(&workspace, &["--json"]);
    assert!(!text(&output).contains(&token), "{}", text(&output));
    assert!(!errors(&output).contains(&token), "{}", errors(&output));
    assert!(errors(&output).contains("local.allowedPaths"));
    assert!(errors(&output).contains("invalid_config"));
    let output = workspace
        .cli()
        .args([
            "localFetch",
            r#"{"queries":[{"path":"missing.txt"}]}"#,
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!errors(&output).contains(&token), "{}", errors(&output));
}

#[test]
fn sensitive_boundary_config_inspector_does_not_echo_credential_keys() {
    let workspace = Workspace::new();
    let token = token();
    home_rc(&workspace, &json!({token.clone():true}).to_string());
    std::fs::write(workspace.home.join(".env"), format!("{token}=public\n")).unwrap();
    for args in [vec!["--json"], vec!["list"], vec!["list", "--json"]] {
        let output = config(&workspace, &args);
        let shown = format!("{}{}", text(&output), errors(&output));
        assert!(!shown.contains(&token), "{shown}");
    }
}
