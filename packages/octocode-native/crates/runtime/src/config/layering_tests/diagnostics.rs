use super::*;

fn protected_storage_diagnostics(out: &ConfigOutput, path: &str) -> usize {
    out.diagnostics
        .iter()
        .filter(|d| d.code == "workspace_config_protected" && d.field_path.as_deref() == Some(path))
        .count()
}

#[test]
fn workspace_cannot_widen_storage_persistence_through_rc_or_env() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"storage":{"mode":"memory"}}"#),
        project_rc: Some(r#"{"storage":{"mode":"persistent"}}"#),
        project_env: Some("OCTOCODE_STORAGE_MODE=persistent"),
        ..NONE
    });
    assert!(!is_persistent_storage_enabled(&out.resolved));
    assert_eq!(protected_storage_diagnostics(&out, "storage.mode"), 1);
    assert!(
        out.dotenv
            .skipped_protected
            .iter()
            .any(|k| k == "OCTOCODE_STORAGE_MODE")
    );
    assert_eq!(out.env_value("OCTOCODE_STORAGE_MODE"), None);
}

#[test]
fn workspace_may_still_opt_out_of_storage_persistence() {
    let rc = resolve(Layers {
        global_rc: Some(r#"{"storage":{"mode":"persistent"}}"#),
        project_rc: Some(r#"{"storage":{"mode":"memory"}}"#),
        ..NONE
    });
    assert!(!is_persistent_storage_enabled(&rc.resolved));
    assert_eq!(protected_storage_diagnostics(&rc, "storage.mode"), 0);

    let env = resolve(Layers {
        global_env: Some("OCTOCODE_STORAGE_MODE=persistent"),
        project_env: Some("OCTOCODE_STORAGE_MODE= Memory "),
        ..NONE
    });
    assert!(!is_persistent_storage_enabled(&env.resolved));
    assert!(env.dotenv.skipped_protected.is_empty());
}

#[test]
fn global_layers_still_set_storage_persistence() {
    let out = resolve(Layers {
        global_env: Some("OCTOCODE_STORAGE_MODE=memory"),
        global_rc: Some(r#"{"storage":{"mode":"persistent"}}"#),
        ..NONE
    });
    assert!(!is_persistent_storage_enabled(&out.resolved));
    assert!(out.dotenv.skipped_protected.is_empty());
    assert_eq!(protected_storage_diagnostics(&out, "storage.mode"), 0);
}

#[test]
fn unparseable_workspace_file_is_skipped_with_a_warning_and_global_applies() {
    for (label, text) in [("parse error", "{not json"), ("not an object", "[1,2]")] {
        let out = resolve(Layers {
            global_rc: Some(r#"{"lsp":{"prewarm":"off"}}"#),
            project_rc: Some(text),
            ..NONE
        });
        assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Off, "{label}");
        assert_eq!(out.source, ConfigSource::Invalid, "{label}");
        assert_eq!(
            out.project_config_path,
            Some(WORKSPACE_RC.into()),
            "{label}"
        );
        let warning = out
            .diagnostics
            .iter()
            .find(|d| d.code == "config_load_error")
            .unwrap_or_else(|| panic!("{label}: {:?}", out.diagnostics));
        assert_eq!(warning.severity, Severity::Warning, "{label}");
        let line = warning.to_string();
        assert!(line.contains(WORKSPACE_RC), "{label}: {line}");
        assert!(line.contains("whole file is ignored"), "{label}: {line}");
    }
}

#[test]
fn schema_error_drops_only_the_bad_field_and_keeps_the_rest_of_the_file() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"local":{"allowedPaths":["/global"]}}"#),
        project_rc: Some(
            r#"{"tools":{"enabled":["localSearch"],"disabled":"many"},"local":{"allowedPaths":["relative"]}}"#,
        ),
        ..NONE
    });
    assert_eq!(
        out.resolved.tools.enabled,
        Some(vec!["localSearch".to_owned()]),
        "valid sibling kept"
    );
    assert_eq!(out.resolved.tools.disabled, None, "bad field → default");
    assert_eq!(
        out.resolved.local.allowed_paths,
        vec!["/global".to_owned()],
        "bad workspace field falls through to the global file"
    );
    assert_eq!(out.source, ConfigSource::File);
    let fields: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "invalid_config")
        .map(|d| {
            assert_eq!(d.severity, Severity::Warning);
            assert_eq!(
                d.source_path.as_deref(),
                Some(std::path::Path::new(WORKSPACE_RC))
            );
            d.field_path.clone().unwrap_or_default()
        })
        .collect();
    let mut fields = fields;
    fields.sort();
    assert_eq!(fields, vec!["local.allowedPaths", "tools.disabled"]);
    let line = out.diagnostics[0].to_string();
    assert!(
        line.starts_with("octocode: config warning: /synthetic/cwd/.octocode/.octocoderc: "),
        "{line}"
    );
    assert!(line.contains("value ignored"), "{line}");
}

#[test]
fn non_object_section_drops_that_section_only() {
    let out = resolve(Layers {
        project_rc: Some(r#"{"network":5,"lsp":{"prewarm":"all"}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::All);
    assert!(!out.resolved.network.allow_private_registry);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.field_path.as_deref() == Some("network"))
    );
}

#[test]
fn invalid_environment_values_warn_with_their_source_and_never_the_value() {
    let out = resolve(Layers {
        env: &[("OCTOCODE_LSP_PREWARM", "private-abc")],
        project_env: Some("OCTOCODE_GITHUB_GRAPHQL=private-maybe"),
        global_env: Some("OCTOCODE_STORAGE_MODE=private-xml"),
        global_rc: Some(r#"{"lsp":{"prewarm":"off"}}"#),
        ..NONE
    });
    assert_eq!(
        out.resolved.lsp.prewarm,
        LspPrewarm::Off,
        "invalid env falls to file"
    );
    let by_field = |path: &str| {
        out.diagnostics
            .iter()
            .find(|d| d.code == "invalid_env_value" && d.field_path.as_deref() == Some(path))
            .unwrap_or_else(|| panic!("{path}: {:?}", out.diagnostics))
    };
    let prewarm = by_field("lsp.prewarm");
    assert_eq!(prewarm.source_path, None);
    assert!(
        prewarm
            .message
            .contains("OCTOCODE_LSP_PREWARM (process environment)")
    );
    assert!(
        prewarm.message.contains("targeted, all, off"),
        "{}",
        prewarm.message
    );
    assert_eq!(
        by_field("github.graphqlEnabled").source_path,
        Some("/synthetic/cwd/.octocode/.env".into())
    );
    assert_eq!(
        by_field("storage.mode").source_path,
        Some("/synthetic/.octocode/.env".into())
    );
    for d in &out.diagnostics {
        assert_eq!(d.severity, Severity::Warning);
        assert!(!d.to_string().contains("private-"), "value leaked: {d}");
    }
}

#[test]
fn blank_environment_values_fall_back_silently() {
    let out = resolve(Layers {
        env: &[("OCTOCODE_LSP_PREWARM", "  ")],
        ..NONE
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn diagnostics_are_never_errors_for_any_file_misconfiguration() {
    for text in [
        "{",
        "[]",
        "null",
        r#"{"version":"x"}"#,
        r#"{"github":{"graphqlEnabled":"yes"}}"#,
        r#"{"storage":{"mode":"disk"}}"#,
    ] {
        for out in [
            resolve(Layers {
                project_rc: Some(text),
                ..NONE
            }),
            resolve(Layers {
                global_rc: Some(text),
                ..NONE
            }),
        ] {
            assert!(
                out.diagnostics
                    .iter()
                    .all(|d| d.severity == Severity::Warning),
                "{text}: {:?}",
                out.diagnostics
            );
            assert!(!out.diagnostics.is_empty(), "{text} must be reported");
        }
    }
}

#[test]
fn invalid_global_file_does_not_discard_a_valid_workspace_file() {
    let out = resolve(Layers {
        global_rc: Some("{broken"),
        project_rc: Some(r#"{"lsp":{"prewarm":"off"}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Off);
    assert_eq!(out.source, ConfigSource::Invalid);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.source_path.as_deref() == Some(std::path::Path::new(GLOBAL_RC)))
    );
}

#[test]
fn unreadable_workspace_file_reports_its_path() {
    let mut i = input(NONE);
    i.project_config_file = FileInput::Unreadable {
        path: WORKSPACE_RC.into(),
        kind: "permission denied".into(),
    };
    let out = resolve_config(&i);
    assert_eq!(out.source, ConfigSource::Invalid);
    assert_eq!(out.project_config_path, Some(WORKSPACE_RC.into()));
    assert!(out.diagnostics.iter().any(|d| d.code == "config_load_error"
        && d.severity == Severity::Warning
        && d.source_path.as_deref() == Some(std::path::Path::new(WORKSPACE_RC))));
}

#[test]
fn empty_workspace_file_is_valid_and_transparent() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"prewarm":"off"}}"#),
        project_rc: Some("  \n"),
        ..NONE
    });
    assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Off);
    assert_eq!(out.source, ConfigSource::File);
    assert!(out.diagnostics.is_empty());
}

#[test]
fn unknown_keys_in_workspace_file_warn_with_its_path() {
    let out = resolve(Layers {
        project_rc: Some(r#"{"surprise":true}"#),
        ..NONE
    });
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning
                && d.source_path.as_deref() == Some(std::path::Path::new(WORKSPACE_RC)))
    );
}
