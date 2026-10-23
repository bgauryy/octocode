use super::*;

#[test]
fn workspace_credential_beats_global_file_but_not_any_dotenv() {
    let global_rc = r#"{"classification":{"api":"from-global-rc"}}"#;
    let project_rc = r#"{"classification":{"api":"from-workspace-rc"}}"#;
    let out = resolve(Layers {
        global_rc: Some(global_rc),
        project_rc: Some(project_rc),
        ..NONE
    });
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("from-workspace-rc")
    );
    let out = resolve(Layers {
        global_rc: Some(global_rc),
        project_rc: Some(project_rc),
        global_env: Some("OCTOCODE_CLASSIFICATION_API=from-dotenv"),
        ..NONE
    });
    assert_ne!(
        out.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("from-workspace-rc"),
        "the global .env must beat the workspace file"
    );
    // Blank credential in the workspace file falls through to the global file.
    let out = resolve(Layers {
        global_rc: Some(global_rc),
        project_rc: Some(r#"{"classification":{"api":"  "}}"#),
        ..NONE
    });
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("from-global-rc")
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
    for field in CONFIG_FIELDS.iter().filter(|field| field.file) {
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
        ConfigFieldKind::StringArray => json!(["/workspace-path"]),
        ConfigFieldKind::Enum => json!(field.values[0]),
    }
}

#[test]
fn every_protected_field_is_ignored_in_workspace_but_honored_globally() {
    // Vacuous until the contract marks a file field's env binding
    // dotenv:"home"/"never"; then it enforces the boundary for each one.
    for field in CONFIG_FIELDS
        .iter()
        .filter(|field| field.file && !super::super::resolver::workspace_file_allowed(field))
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
            "GITHUB_API_URL=https://evil.example/api\nOCTOCODE_LSP_CONFIG=/repo/evil.json\nALLOWED_PATHS=/\nOCTOCODE_CLASSIFICATION_API_HOST=https://evil.example",
        ),
        project_rc: Some(
            r#"{"github":{"apiUrl":"https://evil.example/api"},"lsp":{"configPath":"/repo/evil.json"},"local":{"allowedPaths":["/"],"workspaceRoot":"/"},"classification":{"apiHost":"https://evil.example"}}"#,
        ),
        ..NONE
    });
    for key in [
        "GITHUB_API_URL",
        "OCTOCODE_LSP_CONFIG",
        "ALLOWED_PATHS",
        "OCTOCODE_CLASSIFICATION_API_HOST",
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
    assert_eq!(out.resolved.lsp.config_path, None);
    assert!(out.resolved.local.allowed_paths.is_empty());
    assert_eq!(out.resolved.local.workspace_root, None);
    for field in [
        "github.apiUrl",
        "lsp.configPath",
        "local.allowedPaths",
        "local.workspaceRoot",
        "classification.apiHost",
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
    let mut value = serde_json::json!({"a":{"b":{"c":1,"d":2}},"top":3});
    assert!(super::super::resolver::remove_path(&mut value, "a.b.c"));
    assert!(super::super::resolver::remove_path(&mut value, "top"));
    assert!(!super::super::resolver::remove_path(
        &mut value,
        "a.b.missing"
    ));
    assert!(!super::super::resolver::remove_path(&mut value, "x.y"));
    assert_eq!(value, serde_json::json!({"a":{"b":{"d":2}}}));
}
