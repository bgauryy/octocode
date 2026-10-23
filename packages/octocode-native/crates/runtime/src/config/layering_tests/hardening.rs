use super::*;

/// A tool name no tool has is reported (non-fatal), naming the entry and the
/// nearest valid name; known names report nothing.
#[test]
fn unknown_tool_names_warn_with_the_nearest_valid_name() {
    let out = resolve(Layers {
        env: &[("TOOLS_TO_RUN", "localSerch,localFetch")],
        global_rc: Some(
            r#"{"tools":{"disabled":["ghSearchCod"]},"mcp":{"deferred":["zzzzzzzz"]}}"#,
        ),
        ..NONE
    });
    let warning = |path: &str| {
        out.diagnostics
            .iter()
            .find(|d| d.code == "unknown_tool_name" && d.field_path.as_deref() == Some(path))
            .unwrap_or_else(|| panic!("{path}: {:?}", out.diagnostics))
    };
    let enabled = warning("tools.enabled");
    assert_eq!(enabled.severity, Severity::Warning);
    assert!(
        enabled.message.contains("\"localSerch\""),
        "{}",
        enabled.message
    );
    assert!(
        enabled.message.contains("did you mean \"localSearch\""),
        "{}",
        enabled.message
    );
    assert!(
        !enabled.message.contains("localFetch"),
        "{}",
        enabled.message
    );
    assert!(
        warning("tools.disabled")
            .message
            .contains("did you mean \"ghSearchCode\"")
    );
    let deferred = warning("mcp.deferred");
    assert!(deferred.message.contains("\"zzzzzzzz\""));
    assert!(
        !deferred.message.contains("did you mean"),
        "{}",
        deferred.message
    );
    let clean = resolve(Layers {
        env: &[("TOOLS_TO_RUN", "localSearch,ghSearchCode")],
        ..NONE
    });
    assert!(clean.diagnostics.is_empty(), "{:?}", clean.diagnostics);
}

/// The classification API root from the environment is validated with the
/// configuration: an invalid value is reported and ignored (the file value,
/// if any, applies), never left for the first provider call to discover.
#[test]
fn an_invalid_classification_api_host_env_value_is_reported_and_skipped() {
    let out = resolve(Layers {
        env: &[("OCTOCODE_CLASSIFICATION_API_HOST", "not-a-url")],
        global_rc: Some(r#"{"classification":{"apiHost":"https://jev.example"}}"#),
        ..NONE
    });
    assert!(has_diagnostic(
        &out,
        "invalid_env_value",
        "classification.apiHost"
    ));
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API_HOST"),
        Some("https://jev.example")
    );
    for d in &out.diagnostics {
        assert!(!d.to_string().contains("not-a-url"), "{d}");
    }
    let valid = resolve(Layers {
        env: &[("OCTOCODE_CLASSIFICATION_API_HOST", "https://ok.example")],
        ..NONE
    });
    assert!(valid.diagnostics.is_empty(), "{:?}", valid.diagnostics);
}

/// A workspace `.env` value for a protected key is reported as skipped even
/// when the home `.env` sets the same key.
#[test]
fn a_dropped_workspace_protected_key_is_reported_when_home_also_sets_it() {
    let out = resolve(Layers {
        global_env: Some("OCTOCODE_STORAGE_MODE=memory"),
        project_env: Some("OCTOCODE_STORAGE_MODE=persistent"),
        ..NONE
    });
    assert_eq!(
        out.dotenv.skipped_workspace_protected,
        vec!["OCTOCODE_STORAGE_MODE"],
        "{:?}",
        out.dotenv
    );
    assert_eq!(value_of(&out, "storage.mode"), serde_json::json!("memory"));
    let view = inspector_data(
        &input(Layers {
            global_env: Some("OCTOCODE_STORAGE_MODE=memory"),
            project_env: Some("OCTOCODE_STORAGE_MODE=persistent"),
            ..NONE
        }),
        &out,
    );
    assert!(
        view.skipped_protected
            .iter()
            .any(|skip| skip.key == "OCTOCODE_STORAGE_MODE"
                && skip.source_path == std::path::Path::new("/synthetic/cwd/.octocode/.env"))
    );
}

/// A blank or invalid enum environment value falls through to the files
/// like every other setting; it never resets a file's value to the default.
#[test]
fn an_invalid_enum_env_value_falls_through_to_the_file() {
    for raw in ["", "xml"] {
        let out = resolve(Layers {
            env: &[("OCTOCODE_LSP_PREWARM", raw)],
            global_rc: Some(r#"{"lsp":{"prewarm":"all"}}"#),
            ..NONE
        });
        assert_eq!(
            value_of(&out, "lsp.prewarm"),
            serde_json::json!("all"),
            "{raw:?}"
        );
    }
}

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

/// Removed settings are named as removed: no alias, no silent acceptance.
#[test]
fn removed_settings_are_reported_as_removed() {
    let file = resolve(Layers {
        global_rc: Some(r#"{"version":1,"classification":{"type":"jev"},"lsp":{"prewarm":"off"}}"#),
        ..NONE
    });
    for path in ["version", "classification.type"] {
        let d = file
            .diagnostics
            .iter()
            .find(|d| d.code == "removed_setting" && d.field_path.as_deref() == Some(path))
            .unwrap_or_else(|| panic!("{path}: {:?}", file.diagnostics));
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.message.contains("removed"), "{}", d.message);
    }
    assert!(
        file.diagnostics
            .iter()
            .all(|d| d.code != UNKNOWN_CONFIG_CODE),
        "{:?}",
        file.diagnostics
    );
    assert_eq!(value_of(&file, "lsp.prewarm"), serde_json::json!("off"));
    assert_eq!(value_of(&file, "version"), serde_json::Value::Null);
    assert_eq!(
        value_of(&file, "classification.type"),
        serde_json::Value::Null
    );
    let env = resolve(Layers {
        env: &[("OCTOCODE_CLASSIFICATION_TYPE", "jev")],
        ..NONE
    });
    let d = env
        .diagnostics
        .iter()
        .find(|d| d.code == "removed_setting")
        .unwrap_or_else(|| panic!("{:?}", env.diagnostics));
    assert!(
        d.message.contains("OCTOCODE_CLASSIFICATION_TYPE"),
        "{}",
        d.message
    );
    assert!(!d.message.contains("jev\""), "{}", d.message);
}

/// Internal limits are code constants, not settings: every layer that still
/// names one gets exactly one `removed_setting` warning per name, the value
/// is never applied or clamped, and the variable leaves the effective
/// environment.
#[test]
fn removed_limits_warn_once_and_never_apply() {
    let out = resolve(Layers {
        env: &[("MAX_RETRIES", "0")],
        project_env: Some("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH=50000\nREQUEST_TIMEOUT=300000"),
        global_rc: Some(
            r#"{"network":{"timeout":5000},"output":{"pagination":{"defaultCharLength":4000}},"storage":{"cloneCache":{"ttl":60000}}}"#,
        ),
        project_rc: Some(r#"{"classification":{"maxChars":1048576},"lsp":{"maxMemoryMb":0}}"#),
        ..NONE
    });
    let removed = |name: &str| {
        out.diagnostics
            .iter()
            .filter(|d| {
                d.code == REMOVED_SETTING_CODE
                    && (d.field_path.as_deref() == Some(name)
                        || (d.field_path.is_none() && d.message.starts_with(name)))
            })
            .count()
    };
    for name in [
        "MAX_RETRIES",
        "OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH",
        "REQUEST_TIMEOUT",
        "network.timeout",
        "output.pagination",
        "storage.cloneCache",
        "classification.maxChars",
        "lsp.maxMemoryMb",
    ] {
        assert_eq!(removed(name), 1, "{name}: {:?}", out.diagnostics);
    }
    for d in &out.diagnostics {
        assert_eq!(d.severity, Severity::Warning, "{d}");
        assert!(
            d.code == REMOVED_SETTING_CODE,
            "only removal warnings, no unknown key, clamp or invalid value: {d}"
        );
        assert!(!d.to_string().contains("50000"), "value leaked: {d}");
    }
    let workspace_env = out
        .diagnostics
        .iter()
        .find(|d| d.message.starts_with("REQUEST_TIMEOUT"))
        .expect("workspace .env removal");
    assert_eq!(
        workspace_env.source_path.as_deref(),
        Some(std::path::Path::new("/synthetic/cwd/.octocode/.env"))
    );
    for name in [
        "MAX_RETRIES",
        "OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH",
        "REQUEST_TIMEOUT",
    ] {
        assert_eq!(out.env_value(name), None, "{name} stays out of the env");
        assert!(!out.dotenv.keys.iter().any(|key| key == name), "{name}");
    }
    let resolved = serde_json::to_value(&out.resolved).expect("resolved");
    for path in [
        "network.timeout",
        "network.maxRetries",
        "output.pagination",
        "storage.cloneCache",
        "classification.maxChars",
        "lsp.maxMemoryMb",
    ] {
        assert_eq!(
            super::super::validation::get_path(&resolved, path),
            None,
            "{path} is not a resolved setting"
        );
    }
}
