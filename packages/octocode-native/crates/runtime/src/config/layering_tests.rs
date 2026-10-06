//! Global + workspace `.octocoderc` layering.
//!
//! Precedence, per field:
//! process env > workspace `.env` > global `.env` > workspace `.octocoderc`
//! > global `.octocoderc` > generated default.
use super::*;
use std::fs;
use std::path::PathBuf;

const HOME: &str = "/synthetic/.octocode";
const WORKSPACE_RC: &str = "/synthetic/cwd/.octocode/.octocoderc";
const GLOBAL_RC: &str = "/synthetic/.octocode/.octocoderc";

fn file(path: &str, text: Option<&str>) -> FileInput {
    match text {
        Some(text) => FileInput::Read {
            path: path.into(),
            text: text.into(),
        },
        None => FileInput::Missing { path: path.into() },
    }
}

struct Layers<'a> {
    env: &'a [(&'a str, &'a str)],
    global_env: Option<&'a str>,
    project_env: Option<&'a str>,
    global_rc: Option<&'a str>,
    project_rc: Option<&'a str>,
}

const NONE: Layers<'static> = Layers {
    env: &[],
    global_env: None,
    project_env: None,
    global_rc: None,
    project_rc: None,
};

fn input(layers: Layers<'_>) -> ConfigInput {
    ConfigInput {
        env: layers
            .env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        cwd: "/synthetic/cwd".into(),
        os_home: "/synthetic".into(),
        trusted_project: false,
        global_env: file("/synthetic/.octocode/.env", layers.global_env),
        project_env: file("/synthetic/cwd/.octocode/.env", layers.project_env),
        config_file: file(GLOBAL_RC, layers.global_rc),
        project_config_file: file(WORKSPACE_RC, layers.project_rc),
        runtime_surface: RuntimeSurface::Cli,
    }
}

fn resolve(layers: Layers<'_>) -> ConfigOutput {
    resolve_config(&input(layers))
}

#[test]
fn workspace_file_alone_applies() {
    let out = resolve(Layers {
        project_rc: Some(r#"{"network":{"timeout":7000}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.network.timeout, 7000.0);
    assert_eq!(out.source, ConfigSource::File);
    assert_eq!(out.config_path, None);
    assert_eq!(out.project_config_path, Some(WORKSPACE_RC.into()));
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn global_file_alone_still_applies() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"network":{"timeout":6000}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.network.timeout, 6000.0);
    assert_eq!(out.config_path, Some(GLOBAL_RC.into()));
    assert_eq!(out.project_config_path, None);
}

#[test]
fn workspace_overrides_global_per_field_and_global_fills_the_rest() {
    let out = resolve(Layers {
        global_rc: Some(
            r#"{"network":{"timeout":6000,"maxRetries":1},"output":{"format":"json"}}"#,
        ),
        project_rc: Some(r#"{"network":{"timeout":7000}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.network.timeout, 7000.0, "workspace wins");
    assert_eq!(
        out.resolved.network.max_retries, 1.0,
        "sibling field in the same section still comes from global"
    );
    assert_eq!(
        out.resolved.output.format, "json",
        "untouched section from global"
    );
}

#[test]
fn every_environment_source_beats_both_files() {
    let rc = r#"{"network":{"timeout":7000}}"#;
    for (label, layers) in [
        (
            "process env",
            Layers {
                env: &[("REQUEST_TIMEOUT", "9000")],
                ..NONE
            },
        ),
        (
            "workspace .env",
            Layers {
                project_env: Some("REQUEST_TIMEOUT=9000"),
                ..NONE
            },
        ),
        (
            "global .env",
            Layers {
                global_env: Some("REQUEST_TIMEOUT=9000"),
                ..NONE
            },
        ),
    ] {
        let out = resolve(Layers {
            global_rc: Some(rc),
            project_rc: Some(rc),
            ..layers
        });
        assert_eq!(out.resolved.network.timeout, 9000.0, "{label} must win");
        assert_eq!(out.source, ConfigSource::Mixed, "{label}");
    }
}

#[test]
fn workspace_null_array_resets_a_global_list() {
    let out = resolve(Layers {
        global_rc: Some(r#"{"tools":{"enabled":["localSearch"]}}"#),
        project_rc: Some(r#"{"tools":{"enabled":null}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.tools.enabled, None);
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
            global_rc: Some(r#"{"network":{"timeout":6000}}"#),
            project_rc: Some(text),
            ..NONE
        });
        assert_eq!(out.resolved.network.timeout, 6000.0, "{label}");
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
            r#"{"network":{"timeout":7000,"maxRetries":"many"},"local":{"allowedPaths":["relative"]}}"#,
        ),
        ..NONE
    });
    assert_eq!(out.resolved.network.timeout, 7000.0, "valid sibling kept");
    assert_eq!(out.resolved.network.max_retries, 3.0, "bad field → default");
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
    assert_eq!(fields, vec!["local.allowedPaths", "network.maxRetries"]);
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
        project_rc: Some(r#"{"network":5,"output":{"format":"json"}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.output.format, "json");
    assert_eq!(out.resolved.network.timeout, 30000.0);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.field_path.as_deref() == Some("network"))
    );
}

#[test]
fn invalid_environment_values_warn_with_their_source_and_never_the_value() {
    let out = resolve(Layers {
        env: &[("REQUEST_TIMEOUT", "private-abc")],
        project_env: Some("OCTOCODE_GITHUB_GRAPHQL=private-maybe"),
        global_env: Some("OCTOCODE_OUTPUT_FORMAT=private-xml"),
        global_rc: Some(r#"{"network":{"timeout":6000}}"#),
        ..NONE
    });
    assert_eq!(
        out.resolved.network.timeout, 6000.0,
        "invalid env falls to file"
    );
    let by_field = |path: &str| {
        out.diagnostics
            .iter()
            .find(|d| d.code == "invalid_env_value" && d.field_path.as_deref() == Some(path))
            .unwrap_or_else(|| panic!("{path}: {:?}", out.diagnostics))
    };
    let timeout = by_field("network.timeout");
    assert_eq!(timeout.source_path, None);
    assert!(
        timeout
            .message
            .contains("REQUEST_TIMEOUT (process environment)")
    );
    assert!(timeout.message.contains("integer"));
    assert_eq!(
        by_field("github.graphqlEnabled").source_path,
        Some("/synthetic/cwd/.octocode/.env".into())
    );
    assert_eq!(
        by_field("output.format").source_path,
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
        env: &[("REQUEST_TIMEOUT", "  ")],
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
        project_rc: Some(r#"{"network":{"timeout":7000}}"#),
        ..NONE
    });
    assert_eq!(out.resolved.network.timeout, 7000.0);
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
        global_rc: Some(r#"{"network":{"timeout":6000}}"#),
        project_rc: Some("  \n"),
        ..NONE
    });
    assert_eq!(out.resolved.network.timeout, 6000.0);
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

#[test]
fn workspace_file_loads_without_project_trust_like_workspace_dotenv() {
    let mut i = input(Layers {
        project_rc: Some(r#"{"network":{"timeout":7000}}"#),
        ..NONE
    });
    for trusted in [false, true] {
        i.trusted_project = trusted;
        assert_eq!(resolve_config(&i).resolved.network.timeout, 7000.0);
    }
}

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
    for (label, layers) in [(
        "global .env",
        Layers {
            global_env: Some("OCTOCODE_CLASSIFICATION_API=from-dotenv"),
            ..NONE
        },
    )] {
        let out = resolve(Layers {
            global_rc: Some(global_rc),
            project_rc: Some(project_rc),
            ..layers
        });
        assert_ne!(
            out.env_value("OCTOCODE_CLASSIFICATION_API"),
            Some("from-workspace-rc"),
            "{label} must beat the workspace file"
        );
    }
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
            super::resolver::workspace_file_allowed(field),
            field.env.iter().all(|b| !PROTECTED_KEYS.contains(&b.name)),
            "{}",
            field.path
        );
    }
    let protected = ConfigFieldSpec {
        env: &[ConfigEnvBinding {
            name: "PATH",
            normalize: None,
            invalid: ConfigInvalidEnv::Skip,
        }],
        ..CONFIG_FIELDS[1]
    };
    assert!(!super::resolver::workspace_file_allowed(&protected));
}

/// A valid file value for any field kind.
fn sample(field: &ConfigFieldSpec) -> serde_json::Value {
    use serde_json::json;
    match field.kind {
        ConfigFieldKind::Boolean => json!(true),
        ConfigFieldKind::Number => json!(field.minimum.unwrap_or(1.0) as i64),
        ConfigFieldKind::Url => json!("https://workspace.example"),
        ConfigFieldKind::Path => json!("/workspace-path"),
        ConfigFieldKind::String => json!("workspace-value"),
        ConfigFieldKind::StringArray => json!(["/workspace-path"]),
        ConfigFieldKind::Enum => json!(field.values[0]),
        ConfigFieldKind::SchemaVersion => json!(1),
    }
}

#[test]
fn every_protected_field_is_ignored_in_workspace_but_honored_globally() {
    // Vacuous until the contract marks a file field's env binding
    // dotenv:"home"/"never"; then it enforces the boundary for each one.
    for field in CONFIG_FIELDS
        .iter()
        .filter(|field| field.file && !super::resolver::workspace_file_allowed(field))
    {
        let mut doc = serde_json::json!({});
        super::resolver::insert_path(&mut doc, field.path, sample(field));
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
    assert!(super::resolver::remove_path(&mut value, "a.b.c"));
    assert!(super::resolver::remove_path(&mut value, "top"));
    assert!(!super::resolver::remove_path(&mut value, "a.b.missing"));
    assert!(!super::resolver::remove_path(&mut value, "x.y"));
    assert_eq!(value, serde_json::json!({"a":{"b":{"d":2}}}));
}

// ─── acquisition (real filesystem) ───────────────────────────────────────────

fn acquire(env: &[(&str, String)], cwd: PathBuf, os_home: PathBuf) -> ConfigInput {
    acquire_config_input(
        env.iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        cwd,
        os_home,
        false,
        RuntimeSurface::Cli,
    )
}

#[test]
fn acquisition_reads_workspace_file_from_cwd_dot_octocode() {
    let root = tempfile::tempdir().expect("create temp dir");
    let (os_home, cwd) = (root.path().join("user"), root.path().join("repo"));
    fs::create_dir_all(os_home.join(".octocode")).expect("home");
    fs::create_dir_all(cwd.join(".octocode")).expect("workspace");
    fs::write(
        os_home.join(".octocode/.octocoderc"),
        r#"{"network":{"timeout":6000,"maxRetries":1}}"#,
    )
    .expect("global rc");
    fs::write(
        cwd.join(".octocode/.octocoderc"),
        "{\n  // workspace override\n  \"network\": { \"timeout\": 7000, },\n}\n",
    )
    .expect("workspace rc");
    let i = acquire(&[], cwd.clone(), os_home);
    assert_eq!(
        i.project_config_file.path(),
        &cwd.join(".octocode/.octocoderc")
    );
    let out = resolve_config(&i);
    assert_eq!(out.resolved.network.timeout, 7000.0);
    assert_eq!(out.resolved.network.max_retries, 1.0);
}

#[test]
fn acquisition_never_reads_the_global_file_twice_when_cwd_is_home() {
    let root = tempfile::tempdir().expect("create temp dir");
    let os_home = root.path().join("user");
    fs::create_dir_all(os_home.join(".octocode")).expect("home");
    fs::write(os_home.join(".octocode/.octocoderc"), "{broken").expect("rc");
    // Default home: cwd = OS home makes <cwd>/.octocode the Octocode home.
    let i = acquire(&[], os_home.clone(), os_home.clone());
    assert!(matches!(i.project_config_file, FileInput::Missing { .. }));
    let out = resolve_config(&i);
    assert_eq!(
        out.diagnostics
            .iter()
            .filter(|d| d.code == "config_load_error")
            .count(),
        1,
        "one broken file must yield one error: {:?}",
        out.diagnostics
    );
    // OCTOCODE_HOME spelled differently but pointing at the same directory.
    let repo = root.path().join("repo");
    fs::create_dir_all(repo.join(".octocode")).expect("workspace");
    let i = acquire(
        &[(
            "OCTOCODE_HOME",
            repo.join("sub/../.octocode").to_string_lossy().into_owned(),
        )],
        repo,
        os_home,
    );
    assert!(matches!(i.project_config_file, FileInput::Missing { .. }));
}
