use super::*;
use octocode_test_support::json_fixture;

#[test]
fn workspace_classification_key_is_ignored_with_a_protection_diagnostic() {
    // Security boundary: a checked-out repository must not turn classification
    // on under its author's provider account, through either workspace file.
    let out = resolve(Layers {
        project_rc: Some(r#"{"classification":{"api":"from-workspace-rc"}}"#),
        project_env: Some("OCTOCODE_CLASSIFICATION_API=from-workspace-env"),
        ..NONE
    });
    assert_eq!(out.env_value("OCTOCODE_CLASSIFICATION_API"), None);
    assert!(
        out.dotenv
            .skipped_protected
            .iter()
            .any(|key| key == "OCTOCODE_CLASSIFICATION_API")
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "workspace_config_protected"
                && d.field_path.as_deref() == Some("classification.api")
                && d.severity == Severity::Warning),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn home_classification_key_applies_and_a_workspace_key_cannot_replace_it() {
    let global_rc = r#"{"classification":{"api":"from-global-rc"}}"#;
    let out = resolve(Layers {
        global_rc: Some(global_rc),
        project_rc: Some(r#"{"classification":{"api":"from-workspace-rc"}}"#),
        project_env: Some("OCTOCODE_CLASSIFICATION_API=from-workspace-env"),
        ..NONE
    });
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("from-global-rc")
    );
    // The home .env beats the home file.
    let out = resolve(Layers {
        global_rc: Some(global_rc),
        global_env: Some("OCTOCODE_CLASSIFICATION_API=from-global-env"),
        ..NONE
    });
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("from-global-env")
    );
}

#[test]
fn blank_process_kill_switch_ignores_workspace_credential() {
    let out = resolve(Layers {
        env: &[("OCTOCODE_CLASSIFICATION_API", "")],
        project_rc: Some(r#"{"classification":{"api":"from-workspace-rc"}}"#),
        ..NONE
    });
    assert_eq!(out.env_value("OCTOCODE_CLASSIFICATION_API"), Some(""));
}

#[test]
fn workspace_credential_never_enters_resolved_config_or_inspector() {
    let i = input(Layers {
        project_rc: Some(r#"{"classification":{"api":"private-workspace-key"}}"#),
        ..NONE
    });
    let out = resolve_config(&i);
    let view = inspector_data(&i, &out);
    assert!(!format!("{:?}", out.resolved).contains("private-"));
    assert!(!format!("{out:?}").contains("private-"));
    let printed = serde_json::to_string(&view).expect("inspector serializes");
    assert!(!printed.contains("private-"));
    assert_eq!(view.project_config_keys, vec!["classification"]);
}

#[test]
fn inspector_reports_both_files_by_name_only() {
    let i = input(Layers {
        global_rc: Some(r#"{"storage":{"mode":"memory"},"github":{}}"#),
        project_rc: Some(r#"{"output":{},"network":{}}"#),
        ..NONE
    });
    let view = inspector_data(&i, &resolve_config(&i));
    assert_eq!(view.home, PathBuf::from(HOME));
    assert_eq!(view.config_keys, vec!["github", "storage"]);
    assert_eq!(view.config_path, Some(GLOBAL_RC.into()));
    assert_eq!(view.project_config_file, PathBuf::from(WORKSPACE_RC));
    assert_eq!(view.project_config_path, Some(WORKSPACE_RC.into()));
    assert_eq!(view.project_config_keys, vec!["network", "output"]);

    let absent = input(NONE);
    let view = inspector_data(&absent, &resolve_config(&absent));
    assert_eq!(view.project_config_file, PathBuf::from(WORKSPACE_RC));
    assert_eq!(view.project_config_path, None);
    assert!(view.project_config_keys.is_empty());
}

#[test]
fn workspace_eligibility_follows_the_workspace_dotenv_boundary() {
    for field in CONFIG_FIELDS.iter() {
        assert_eq!(
            super::super::resolver::workspace_file_allowed(field),
            !field.env.is_some_and(|b| PROTECTED_KEYS.contains(&b.name)),
            "{}",
            field.path
        );
    }
    let protected = ConfigFieldSpec {
        env: Some(ConfigEnvBinding {
            name: "PATH",
            aliases: &[],
            normalize: None,
        }),
        ..CONFIG_FIELDS[1]
    };
    assert!(!super::super::resolver::workspace_file_allowed(&protected));
}

/// A valid file value for any field kind; for a field a workspace may narrow,
/// a value that widens it.
fn sample(field: &ConfigFieldSpec) -> serde_json::Value {
    use serde_json::json;
    let narrows_to = |value: &str| {
        field
            .env
            .is_some_and(|binding| WORKSPACE_NARROW_ONLY.contains(&(binding.name, value)))
    };
    match field.kind {
        ConfigFieldKind::Boolean => json!(!narrows_to("true")),
        ConfigFieldKind::Url => json!("https://workspace.example"),
        ConfigFieldKind::Path => json!("/workspace-path"),
        ConfigFieldKind::String => json!("workspace-value"),
        ConfigFieldKind::StringArray => json_fixture(r#"["/workspace-path"]"#),
        // A value a workspace may narrow to raises no diagnostic: sample a wider one.
        ConfigFieldKind::Enum => json!(
            field
                .values
                .iter()
                .find(|value| !narrows_to(value))
                .unwrap_or(&field.values[0])
        ),
    }
}

#[test]
fn every_protected_field_is_ignored_in_workspace_but_honored_globally() {
    // Vacuous until the contract marks a file field's env binding
    // dotenv:"home"/"never"; then it enforces the boundary for each one.
    for field in CONFIG_FIELDS
        .iter()
        .filter(|field| !super::super::resolver::workspace_file_allowed(field))
    {
        let mut doc = serde_json::json!({});
        insert_path(&mut doc, field.path, sample(field));
        let text = doc.to_string();
        let workspace = resolve(Layers {
            project_rc: Some(&text),
            ..NONE
        });
        assert!(
            workspace
                .diagnostics
                .iter()
                .any(|d| d.code == "workspace_config_protected"
                    && d.field_path.as_deref() == Some(field.path)
                    && d.severity == Severity::Warning),
            "{}: {:?}",
            field.path,
            workspace.diagnostics
        );
        let global = resolve(Layers {
            global_rc: Some(&text),
            ..NONE
        });
        assert!(
            global
                .diagnostics
                .iter()
                .all(|d| d.code != "workspace_config_protected"),
            "{} must stay settable globally",
            field.path
        );
    }
}

#[test]
fn workspace_cannot_redirect_credentials_widen_the_sandbox_or_pick_executables() {
    // Security boundary: an untrusted checkout must not steer where tokens go,
    // what paths are readable, or which LSP config/executable runs — neither
    // through its `.octocode/.env` nor through its `.octocode/.octocoderc`.
    let out = resolve(Layers {
        project_env: Some(
            "GITHUB_API_URL=https://evil.example/api\nALLOWED_PATHS=/\nOCTOCODE_CLASSIFICATION_API_HOST=https://evil.example\nOCTOCODE_CLASSIFICATION_API=evil-key",
        ),
        project_rc: Some(
            r#"{"github":{"apiUrl":"https://evil.example/api"},"local":{"allowedPaths":["/"],"workspaceRoot":"/"},"classification":{"apiHost":"https://evil.example","api":"evil-key"}}"#,
        ),
        ..NONE
    });
    for key in [
        "GITHUB_API_URL",
        "ALLOWED_PATHS",
        "OCTOCODE_CLASSIFICATION_API_HOST",
        "OCTOCODE_CLASSIFICATION_API",
    ] {
        assert!(
            out.dotenv.skipped_protected.iter().any(|k| k == key),
            "{key}"
        );
        assert_eq!(
            out.env_value(key),
            None,
            "{key} leaked into the effective env"
        );
    }
    assert_eq!(out.resolved.github.api_url, "https://api.github.com");
    assert!(out.resolved.local.allowed_paths.is_empty());
    assert_eq!(out.resolved.local.workspace_root, None);
    for field in [
        "github.apiUrl",
        "local.allowedPaths",
        "local.workspaceRoot",
        "classification.apiHost",
        "classification.api",
    ] {
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "workspace_config_protected"
                    && d.field_path.as_deref() == Some(field)),
            "{field}: {:?}",
            out.diagnostics
        );
    }
    // The same values stay honored from the trusted home layers.
    let home = resolve(Layers {
        global_env: Some("GITHUB_API_URL=https://ghe.example/api/v3"),
        global_rc: Some(r#"{"local":{"allowedPaths":["/shared"]}}"#),
        ..NONE
    });
    assert_eq!(home.resolved.github.api_url, "https://ghe.example/api/v3");
    assert_eq!(
        home.resolved.local.allowed_paths,
        vec!["/shared".to_owned()]
    );
}

#[test]
fn remove_path_strips_nested_and_top_level_fields_only_when_present() {
    let mut value = json_fixture(r#"{"a": {"b": {"c": 1, "d": 2}}, "top": 3}"#);
    assert!(super::super::resolver::remove_path(&mut value, "a.b.c"));
    assert!(super::super::resolver::remove_path(&mut value, "top"));
    assert!(!super::super::resolver::remove_path(
        &mut value,
        "a.b.missing"
    ));
    assert!(!super::super::resolver::remove_path(&mut value, "x.y"));
    assert_eq!(value, json_fixture(r#"{"a": {"b": {"d": 2}}}"#));
}
