//! Every contract setting through every layer. The samples derive from each
//! field's own type, so a new setting is covered without touching this file.
use super::super::resolver::workspace_setting_allowed;
use super::*;
use serde_json::{Value, json};

/// A valid `.octocoderc` value that differs from the field's default.
fn file_sample(field: &ConfigFieldSpec) -> Value {
    let default: Value = serde_json::from_str(field.default_json).expect("default");
    match field.kind {
        ConfigFieldKind::Boolean => json!(!default.as_bool().unwrap_or(false)),
        ConfigFieldKind::Enum => json!(
            field
                .values
                .iter()
                .find(|value| default.as_str() != Some(**value))
                .expect("an enum has a non-default value")
        ),
        ConfigFieldKind::Url => json!("https://settings-matrix.example"),
        ConfigFieldKind::Path => json!("/synthetic/matrix"),
        ConfigFieldKind::String => json!("matrix-value"),
        ConfigFieldKind::StringArray if field.item_path => json!(["/synthetic/matrix"]),
        ConfigFieldKind::StringArray => json!(["localSearch"]),
    }
}

/// `file_sample` as its environment variable spells it.
fn env_sample(field: &ConfigFieldSpec) -> String {
    match file_sample(field) {
        Value::String(text) => text,
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    }
}

fn rc_with(field: &ConfigFieldSpec, value: Value) -> String {
    let mut root = json!({});
    insert_path(&mut root, field.path, value);
    root.to_string()
}

fn settings() -> impl Iterator<Item = &'static ConfigFieldSpec> {
    CONFIG_FIELDS.iter().filter(|field| field.resolved)
}

#[test]
fn no_source_leaves_every_setting_on_its_contract_default() {
    let out = resolve(NONE);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    for field in settings() {
        let default: Value = serde_json::from_str(field.default_json).expect("default");
        assert_eq!(value_of(&out, field.path), default, "{}", field.path);
        assert_eq!(
            out.setting_source(field.path),
            SettingSource::Default,
            "{}",
            field.path
        );
    }
}

#[test]
fn the_home_octocoderc_sets_every_file_setting() {
    for field in settings() {
        let sample = file_sample(field);
        let rc = rc_with(field, sample.clone());
        let out = resolve(Layers {
            global_rc: Some(&rc),
            ..NONE
        });
        assert_eq!(value_of(&out, field.path), sample, "{}", field.path);
        assert_eq!(
            out.setting_source(field.path),
            SettingSource::HomeFile,
            "{}",
            field.path
        );
        assert!(
            out.diagnostics.is_empty(),
            "{}: {:?}",
            field.path,
            out.diagnostics
        );
    }
}

/// A workspace file holds a setting only where a workspace `.env` could: a
/// repository never widens a home-only setting through one file or the other.
#[test]
fn a_workspace_octocoderc_sets_a_setting_only_where_trust_allows() {
    for field in settings() {
        let sample = file_sample(field);
        let rc = rc_with(field, sample.clone());
        let out = resolve(Layers {
            project_rc: Some(&rc),
            ..NONE
        });
        if workspace_setting_allowed(field, &sample) {
            assert_eq!(value_of(&out, field.path), sample, "{}", field.path);
            assert_eq!(
                out.setting_source(field.path),
                SettingSource::WorkspaceFile,
                "{}",
                field.path
            );
        } else {
            let default: Value = serde_json::from_str(field.default_json).expect("default");
            assert_eq!(value_of(&out, field.path), default, "{}", field.path);
            assert!(
                has_diagnostic(&out, "workspace_config_protected", field.path),
                "{}: {:?}",
                field.path,
                out.diagnostics
            );
        }
    }
}

#[test]
fn a_workspace_octocoderc_beats_the_home_one_per_setting() {
    for field in settings().filter(|field| workspace_setting_allowed(field, &file_sample(field))) {
        let sample = file_sample(field);
        // A home value of the field's own type that the workspace must beat.
        let home = match field.kind {
            ConfigFieldKind::Boolean | ConfigFieldKind::Enum => {
                serde_json::from_str(field.default_json).expect("default")
            }
            ConfigFieldKind::Url => json!("https://home-matrix.example"),
            ConfigFieldKind::Path => json!("/synthetic/home-matrix"),
            ConfigFieldKind::String => json!("home-value"),
            ConfigFieldKind::StringArray if field.item_path => json!(["/synthetic/home-matrix"]),
            ConfigFieldKind::StringArray => json!(["localFetch"]),
        };
        let home_rc = rc_with(field, home);
        let project_rc = rc_with(field, sample.clone());
        let out = resolve(Layers {
            global_rc: Some(&home_rc),
            project_rc: Some(&project_rc),
            ..NONE
        });
        assert_eq!(value_of(&out, field.path), sample, "{}", field.path);
        assert_eq!(
            out.setting_source(field.path),
            SettingSource::WorkspaceFile,
            "{}",
            field.path
        );
    }
}

/// Process, then workspace `.env`, then home `.env`, all above both files.
/// The file holds the type's other value, so a file win is visible.
#[test]
fn each_environment_layer_beats_the_octocoderc_files_for_every_setting() {
    for field in settings() {
        let Some(binding) = field.env else { continue };
        let sample = file_sample(field);
        let line = format!("{}={}", binding.name, env_sample(field));
        let process = [(binding.name, env_sample(field))];
        let process: Vec<(&str, &str)> = process
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let opposite = serde_json::from_str(field.default_json).expect("default");
        let rc = rc_with(field, opposite);
        for (label, layers, source) in [
            (
                "process",
                Layers {
                    env: &process,
                    ..NONE
                },
                SettingSource::Process,
            ),
            (
                "home .env",
                Layers {
                    global_env: Some(&line),
                    ..NONE
                },
                SettingSource::HomeEnv,
            ),
        ] {
            let out = resolve(Layers {
                global_rc: Some(&rc),
                ..layers
            });
            // Blank samples never arise, so a skipped value is a defect here.
            assert_eq!(value_of(&out, field.path), sample, "{label} {}", field.path);
            assert_eq!(
                out.setting_source(field.path),
                source,
                "{label} {}",
                field.path
            );
        }
    }
}

/// A URL that embeds a secret is refused on save. One already in a file stays
/// resolved, so its reader refuses it and the call fails closed: falling back to
/// the default host would send the same credential somewhere else. Either way
/// the report never shows the secret.
#[test]
fn a_url_that_embeds_a_secret_is_refused_on_save_and_fails_closed_when_hand_edited() {
    let secret = "matrix-secret";
    for raw in [
        format!("https://user:{secret}@api.example"),
        format!("https://{secret}@api.example"),
        format!("https://api.example/?token={secret}"),
        format!("https://api.example/#{secret}"),
    ] {
        for field in settings().filter(|field| field.kind == ConfigFieldKind::Url) {
            let binding = field.env.expect("URL settings have a variable");
            let error =
                super::super::manage::validate_env_value(binding.name, &raw).expect_err(&raw);
            assert!(!error.contains(secret), "{error}");

            let process = [(binding.name, raw.as_str())];
            let from_env = resolve(Layers {
                env: &process,
                ..NONE
            });
            let rc = rc_with(field, json!(raw));
            let from_file = resolve(Layers {
                global_rc: Some(&rc),
                ..NONE
            });
            for out in [from_env, from_file] {
                assert_eq!(value_of(&out, field.path), json!(raw), "{}", field.path);
                let report = serde_json::to_string(&super::super::setting_report(&out, field))
                    .expect("report");
                assert!(!report.contains(secret), "{report}");
            }
        }
    }
}

/// The classification host is an ordinary setting: readable, reported with
/// its source, and home-only so a repository cannot redirect the key.
#[test]
fn the_classification_host_is_a_resolved_home_only_setting() {
    let host = "classification.apiHost";
    let rc = r#"{"classification":{"apiHost":"https://home.example"}}"#;
    let out = resolve(Layers {
        global_rc: Some(rc),
        ..NONE
    });
    assert_eq!(value_of(&out, host), json!("https://home.example"));
    assert_eq!(out.setting_source(host), SettingSource::HomeFile);

    let line = "OCTOCODE_CLASSIFICATION_API_HOST=https://env.example";
    let from_dotenv = resolve(Layers {
        global_env: Some(line),
        global_rc: Some(rc),
        ..NONE
    });
    assert_eq!(value_of(&from_dotenv, host), json!("https://env.example"));
    assert_eq!(from_dotenv.setting_source(host), SettingSource::HomeEnv);

    let attacker = r#"{"classification":{"apiHost":"https://attacker.example"}}"#;
    let workspace = resolve(Layers {
        project_env: Some("OCTOCODE_CLASSIFICATION_API_HOST=https://attacker.example"),
        project_rc: Some(attacker),
        ..NONE
    });
    assert_eq!(value_of(&workspace, host), Value::Null);
    assert!(has_diagnostic(
        &workspace,
        "workspace_config_protected",
        host
    ));
    assert!(
        workspace
            .dotenv
            .skipped_protected
            .iter()
            .any(|key| key == "OCTOCODE_CLASSIFICATION_API_HOST"),
        "{:?}",
        workspace.dotenv
    );

    for blank in ["", "   "] {
        let process = [("OCTOCODE_CLASSIFICATION_API_HOST", blank)];
        let out = resolve(Layers {
            env: &process,
            global_rc: Some(rc),
            ..NONE
        });
        assert_eq!(value_of(&out, host), json!("https://home.example"));
    }
}

/// A reader always learns what an unset setting does: every `null` default
/// states its meaning, in `config list` and the docs alike.
#[test]
fn every_setting_states_its_default() {
    for field in CONFIG_FIELDS {
        assert_ne!(
            field.default_value(),
            Value::Null,
            "{} needs a displayDefault",
            field.path
        );
    }
}

/// The host and model the docs and `config list` name are the ones a call uses.
#[test]
fn the_displayed_classification_defaults_are_the_providers_own() {
    let defaults = crate::providers::classification::provider().defaults();
    assert_eq!(
        field::CLASSIFICATION_API_HOST.display_default,
        Some(defaults.host)
    );
    assert_eq!(
        field::CLASSIFICATION_MODEL.display_default,
        Some(defaults.model)
    );
}
