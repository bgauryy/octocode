use super::*;

#[test]
fn workspace_file_alone_applies() {
    let out = resolve(Layers {
        project_rc: Some(r#"{"lsp":{"prewarm":"off"}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Off);
    assert_eq!(out.source, ConfigSource::File);
    assert_eq!(out.config_path, None);
    assert_eq!(out.project_config_path, Some(WORKSPACE_RC.into()));
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn global_file_alone_still_applies() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"prewarm":"all"}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::All);
    assert_eq!(out.config_path, Some(GLOBAL_RC.into()));
    assert_eq!(out.project_config_path, None);
}

#[test]
fn workspace_overrides_global_per_field_and_global_fills_the_rest() {
    let out = resolve(Layers {
        global_rc: Some(
            r#"{"tools":{"enabled":["localSearch"],"disabled":["astSearch"]},"lsp":{"prewarm":"all"}}"#,
        ),
        project_rc: Some(r#"{"tools":{"enabled":["localFetch"]}}"#),
        ..NONE
    });
    assert_eq!(
        out.resolved.tools.enabled,
        Some(vec!["localFetch".to_owned()]),
        "workspace wins"
    );
    assert_eq!(
        out.resolved.tools.disabled,
        Some(vec!["astSearch".to_owned()]),
        "sibling field in the same section still comes from global"
    );
    assert_eq!(
        out.resolved.lsp.prewarm,
        LspPrewarm::All,
        "untouched section from global"
    );
}

#[test]
fn every_environment_source_beats_both_files() {
    let rc = r#"{"lsp":{"prewarm":"all"}}"#;
    for (label, layers) in [
        (
            "process env",
            Layers {
                env: &[("OCTOCODE_LSP_PREWARM", "off")],
                ..NONE
            },
        ),
        (
            "workspace .env",
            Layers {
                project_env: Some("OCTOCODE_LSP_PREWARM=off"),
                ..NONE
            },
        ),
        (
            "global .env",
            Layers {
                global_env: Some("OCTOCODE_LSP_PREWARM=off"),
                ..NONE
            },
        ),
    ] {
        let out = resolve(Layers {
            global_rc: Some(rc),
            project_rc: Some(rc),
            ..layers
        });
        assert_eq!(
            out.resolved.lsp.prewarm,
            LspPrewarm::Off,
            "{label} must win"
        );
        assert_eq!(out.source, ConfigSource::Mixed, "{label}");
    }
}

/// The contract's rule: `null` means "use the next source or the generated
/// default" — a workspace `null` does not reset a global list.
#[test]
fn workspace_null_falls_through_to_the_global_value() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"tools":{"enabled":["localSearch"]}}"#),
        project_rc: Some(r#"{"tools":{"enabled":null}}"#),
        ..NONE
    });
    assert_eq!(
        out.resolved.tools.enabled,
        Some(vec!["localSearch".to_owned()])
    );
}

/// `null` for any setting of any kind falls through to its generated
/// default, never breaks resolution, and never touches another setting.
#[test]
fn null_falls_through_for_every_config_kind() {
    for field in CONFIG_FIELDS
        .iter()
        .filter(|field| field.file && field.resolved)
    {
        let mut doc = serde_json::json!({"lsp": {"prewarm": "off"}});
        insert_path(&mut doc, field.path, serde_json::Value::Null);
        let text = doc.to_string();
        let out = resolve(Layers {
            global_rc: Some(&text),
            ..NONE
        });
        let default: serde_json::Value =
            serde_json::from_str(field.default_json).expect("generated default");
        let resolved = super::super::get_config_value(&out.resolved, field.path)
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(resolved, default, "{}", field.path);
        if field.path != "lsp.prewarm" {
            assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Off, "{}", field.path);
        }
        assert!(
            out.diagnostics.is_empty(),
            "{}: {:?}",
            field.path,
            out.diagnostics
        );
    }
}

/// A null list in a disabled-local config once broke resolution and
/// replaced every setting with zero values (an empty provider type).
#[test]
fn a_null_list_keeps_every_other_setting() {
    let out = resolve(Layers {
        global_rc: Some(
            r#"{"local":{"enabled":false,"allowedPaths":null},"lsp":{"prewarm":"off"}}"#,
        ),
        ..NONE
    });
    assert!(!out.resolved.local.enabled);
    assert!(out.resolved.local.allowed_paths.is_empty());
    assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Off);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn workspace_array_replaces_instead_of_concatenating() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"tools":{"disabled":["ghSearchCode","astSearch"]}}"#),
        project_rc: Some(r#"{"tools":{"disabled":["lspSearch"]}}"#),
        ..NONE
    });
    assert_eq!(
        out.resolved.tools.disabled,
        Some(vec!["lspSearch".to_owned()])
    );
}
