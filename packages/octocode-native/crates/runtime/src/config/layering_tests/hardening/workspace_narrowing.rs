use super::*;

/// A workspace may narrow, never widen, what the home layers set for local
/// tools and email redaction.
#[test]
fn a_workspace_cannot_widen_local_tools_or_email_redaction() {
    let widened = resolve(Layers {
        global_rc: Some(r#"{"local":{"enabled":false},"output":{"redactEmails":true}}"#),
        project_env: Some("OCTOCODE_ENABLE_LOCAL=true\nOCTOCODE_REDACT_EMAILS=false"),
        project_rc: Some(r#"{"local":{"enabled":true},"output":{"redactEmails":false}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&widened, "local.enabled"),
        serde_json::json!(false)
    );
    assert_eq!(
        value_of(&widened, "output.redactEmails"),
        serde_json::json!(true)
    );
    for key in ["OCTOCODE_ENABLE_LOCAL", "OCTOCODE_REDACT_EMAILS"] {
        assert!(
            widened.dotenv.skipped_protected.iter().any(|k| k == key),
            "{key}"
        );
    }
    for path in ["local.enabled", "output.redactEmails"] {
        assert!(
            has_diagnostic(&widened, "workspace_config_protected", path),
            "{path}"
        );
    }
    let narrowed = resolve(Layers {
        project_env: Some("OCTOCODE_ENABLE_LOCAL=false\nOCTOCODE_REDACT_EMAILS=true"),
        ..NONE
    });
    assert_eq!(
        value_of(&narrowed, "local.enabled"),
        serde_json::json!(false)
    );
    assert_eq!(
        value_of(&narrowed, "output.redactEmails"),
        serde_json::json!(true)
    );
    assert!(narrowed.dotenv.skipped_protected.is_empty());
}

/// A cloned repository cannot open credential files, by either file; it can
/// only narrow a home that allows them. Saved work follows the effective value.
#[test]
fn a_workspace_cannot_allow_credential_files() {
    let widened = resolve(Layers {
        project_env: Some("OCTOCODE_CREDENTIAL_FILES=allow"),
        project_rc: Some(r#"{"security":{"credentialFiles":"allow"}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&widened, "security.credentialFiles"),
        serde_json::json!("withhold")
    );
    assert!(has_diagnostic(
        &widened,
        "workspace_config_protected",
        "security.credentialFiles"
    ));
    assert!(saves_local_results(&widened.resolved));

    let home = resolve(Layers {
        global_rc: Some(r#"{"security":{"credentialFiles":"allow"}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&home, "security.credentialFiles"),
        serde_json::json!("allow")
    );
    assert!(!saves_local_results(&home.resolved));
    // Provider answers and clones hold no credential file, and stay on disk.
    assert!(is_persistent_storage_enabled(&home.resolved));

    let narrowed = resolve(Layers {
        global_rc: Some(r#"{"security":{"credentialFiles":"allow"}}"#),
        project_rc: Some(r#"{"security":{"credentialFiles":"withhold"}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&narrowed, "security.credentialFiles"),
        serde_json::json!("withhold")
    );
}

/// A cloned repository cannot trust itself to run a language server, by either
/// file; it can only narrow a home that does.
#[test]
fn a_workspace_cannot_opt_in_to_language_servers_on_fetched_code() {
    let widened = resolve(Layers {
        project_env: Some("OCTOCODE_LSP_TRUST_FETCHED_CODE=true"),
        project_rc: Some(r#"{"lsp":{"trustFetchedCode":true}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&widened, "lsp.trustFetchedCode"),
        serde_json::json!(false)
    );
    assert!(has_diagnostic(
        &widened,
        "workspace_config_protected",
        "lsp.trustFetchedCode"
    ));

    let home = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"trustFetchedCode":true}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&home, "lsp.trustFetchedCode"),
        serde_json::json!(true)
    );

    let narrowed = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"trustFetchedCode":true}}"#),
        project_rc: Some(r#"{"lsp":{"trustFetchedCode":false}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&narrowed, "lsp.trustFetchedCode"),
        serde_json::json!(false)
    );
}

/// A repository cannot allow its own Cargo build scripts and proc macros to
/// run through lspSearch, by either file; it can only narrow a home that does.
#[test]
fn a_workspace_cannot_allow_lsp_build_scripts() {
    let widened = resolve(Layers {
        project_env: Some("OCTOCODE_LSP_ALLOW_BUILD_SCRIPTS=true"),
        project_rc: Some(r#"{"lsp":{"allowBuildScripts":true}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&widened, "lsp.allowBuildScripts"),
        serde_json::json!(false)
    );
    assert!(has_diagnostic(
        &widened,
        "workspace_config_protected",
        "lsp.allowBuildScripts"
    ));

    let home = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"allowBuildScripts":true}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&home, "lsp.allowBuildScripts"),
        serde_json::json!(true)
    );
    let home_env = resolve(Layers {
        global_env: Some("OCTOCODE_LSP_ALLOW_BUILD_SCRIPTS=true"),
        ..NONE
    });
    assert_eq!(
        value_of(&home_env, "lsp.allowBuildScripts"),
        serde_json::json!(true)
    );

    let narrowed = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"allowBuildScripts":true}}"#),
        project_rc: Some(r#"{"lsp":{"allowBuildScripts":false}}"#),
        ..NONE
    });
    assert_eq!(
        value_of(&narrowed, "lsp.allowBuildScripts"),
        serde_json::json!(false)
    );
}
