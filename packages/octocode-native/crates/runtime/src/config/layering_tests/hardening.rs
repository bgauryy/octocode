use super::*;

/// A tool name no tool has is reported (non-fatal), naming the entry and the
/// nearest valid name; known names report nothing.
#[test]
fn unknown_tool_names_warn_with_the_nearest_valid_name() {
    let out = resolve(Layers {
        env: &[("TOOLS_TO_RUN", "localSerch,localFetch")],
        global_rc: Some(r#"{"tools":{"disabled":["ghSearchCod","zzzzzzzz"]}}"#),
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
    assert!(warning("tools.disabled").message.contains("\"zzzzzzzz\""));
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

#[test]
fn unknown_settings_warn_without_exposing_values_or_overriding_valid_fields() {
    let out = resolve(Layers {
        global_rc: Some(
            r#"{"version":"private-marker","unknownSection":{"secret":"private-marker"},"lsp":{"prewarm":"off","unknownKey":"private-marker"}}"#,
        ),
        project_rc: Some(r#"{"lsp":{"prewarm":"all"}}"#),
        ..NONE
    });
    assert_eq!(value_of(&out, "lsp.prewarm"), serde_json::json!("all"));
    assert_eq!(out.diagnostics.len(), 3, "{:?}", out.diagnostics);
    for diagnostic in &out.diagnostics {
        assert_eq!(diagnostic.code, UNKNOWN_CONFIG_CODE);
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert!(!diagnostic.to_string().contains("private-marker"));
        assert_eq!(
            diagnostic.source_path.as_deref(),
            Some(std::path::Path::new(GLOBAL_RC))
        );
    }
    for path in ["version", "unknownSection", "lsp.unknownKey"] {
        assert_eq!(value_of(&out, path), serde_json::Value::Null);
    }
}

#[test]
fn unbound_environment_variables_preserve_values_without_becoming_settings() {
    let baseline = resolve(NONE);
    let out = resolve(Layers {
        env: &[("UNBOUND_CUSTOM_SETTING", "process-marker")],
        global_env: Some("UNBOUND_CUSTOM_SETTING=home-marker\nANOTHER_CUSTOM_KEY=home-marker"),
        project_env: Some("ANOTHER_CUSTOM_KEY=workspace-marker"),
        ..NONE
    });
    assert_eq!(
        serde_json::to_value(&out.resolved).unwrap(),
        serde_json::to_value(&baseline.resolved).unwrap()
    );
    assert_eq!(
        out.env_value("UNBOUND_CUSTOM_SETTING"),
        Some("process-marker")
    );
    assert_eq!(
        out.env_value("ANOTHER_CUSTOM_KEY"),
        Some("workspace-marker")
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}
