use super::*;

#[test]
fn inspector_exposes_names_and_counts_without_values() {
    let mut i = input(
        BTreeMap::from([("HOST".into(), "private-host-value".into())]),
        Some("{\"storage\":{\"mode\":\"memory\"},\"local\":{}}"),
    );
    i.global_env = FileInput::Read {
        path: "/synthetic/home/.env".into(),
        text: "GLOBAL=private-global-value".into(),
    };
    i.project_env = FileInput::Read {
        path: "/synthetic/cwd/.octocode/.env".into(),
        text: "PROJECT=private-project-value".into(),
    };
    let out = resolve_config(&i);
    let view = inspector_data(&i, &out);
    assert_eq!((view.global_key_count, view.project_key_count), (1, 1));
    assert_eq!(view.storage_mode, "memory");
    assert!(
        out.effective_env
            .get("HOST")
            .is_some_and(|value| !value.is_empty())
    );
    let printed = serde_json::to_string(&view).expect("test fixture operation should succeed");
    assert!(!printed.contains("private-"));
    assert!(!format!("{out:?}").contains("private-"));
    assert_eq!(view.config_keys, vec!["local", "storage"]);
}
#[test]
fn inspector_reports_skipped_env_keys_with_source_file() {
    let mut i = input(
        BTreeMap::from([("EXISTING".into(), "from-process".into())]),
        None,
    );
    i.global_env = FileInput::Read {
        path: "/synthetic/home/.env".into(),
        text: "NODE_OPTIONS=private-option-value".into(),
    };
    i.project_env = FileInput::Read {
        path: "/synthetic/cwd/.octocode/.env".into(),
        text: "EXISTING=private-project-value".into(),
    };
    let out = resolve_config(&i);
    let view = inspector_data(&i, &out);
    assert_eq!(
        view.skipped_protected,
        vec![EnvSkip {
            key: "NODE_OPTIONS".into(),
            source_path: "/synthetic/home/.env".into()
        }]
    );
    assert_eq!(
        view.skipped_existing,
        vec![EnvSkip {
            key: "EXISTING".into(),
            source_path: "/synthetic/cwd/.octocode/.env".into()
        }]
    );
    // Key names only — never values.
    let printed = serde_json::to_string(&view).expect("test fixture operation should succeed");
    assert!(!printed.contains("private-"));
}
#[test]
fn local_switch_has_one_env_name() {
    let off = resolve_sections(
        &[],
        &BTreeMap::from([("OCTOCODE_ENABLE_LOCAL".into(), "false".into())]),
    );
    assert!(!off.unwrap_or_default().local.enabled);
    // The retired `ENABLE_LOCAL` spelling changes nothing.
    let retired = resolve_sections(
        &[],
        &BTreeMap::from([("ENABLE_LOCAL".into(), "false".into())]),
    );
    assert!(retired.unwrap_or_default().local.enabled);
    let env = BTreeMap::from([("OCTOCODE_ENABLE_LOCAL".into(), "true".into())]);
    assert_eq!(resolve_config(&input(env, None)).source, ConfigSource::Env);
}
#[test]
fn classification_key_falls_back_to_config_file_without_leaking() {
    let file = "{\"classification\":{\"api\":\"jev-secret-from-file\",\"apiHost\":\"https://jev.example.com\"}}";
    let out = resolve_config(&input(BTreeMap::new(), Some(file)));
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("jev-secret-from-file")
    );
    assert_eq!(
        out.env_value("OCTOCODE_CLASSIFICATION_API_HOST"),
        Some("https://jev.example.com")
    );
    // Never lands in ResolvedConfig, so `config get classification.api` can't echo it back.
    assert_eq!(get_config_value(&out.resolved, "classification.api"), None);
    assert!(!format!("{out:?}").contains("jev-secret-from-file"));
    // Real env still wins over the file fallback.
    let env = BTreeMap::from([(
        "OCTOCODE_CLASSIFICATION_API".into(),
        "jev-secret-from-env".into(),
    )]);
    let mixed = resolve_config(&input(env, Some(file)));
    assert_eq!(
        mixed.env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("jev-secret-from-env")
    );
    // `classification` is a recognized section, not an "unknown configuration key".
    assert!(
        validation::config_issues(&json!({"classification": {"api": "x"}}))
            .0
            .is_empty()
    );
}
#[test]
fn trusted_dotenv_credentials_use_process_then_project_then_home() {
    for key in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "OCTOCODE_CLASSIFICATION_API",
        "OCTOCODE_CLASSIFICATION_API_HOST",
    ] {
        for (explicit, expected) in [
            (None, "project-secret"),
            (Some("process-secret"), "process-secret"),
        ] {
            // Home-trusted keys (e.g. an endpoint) never come from a workspace.
            let expected = if expected == "project-secret" && HOME_TRUSTED_ENV_KEYS.contains(&key) {
                "home-secret"
            } else {
                expected
            };
            let value = |label: &str| valid_value(key, label);
            let expected = value(expected);
            let env = explicit
                .map(|label| BTreeMap::from([(key.into(), value(label))]))
                .unwrap_or_default();
            let mut i = input(env, None);
            i.global_env = FileInput::Read {
                path: "/synthetic/home/.env".into(),
                text: format!("{key}={}\nHOME_ONLY=home", value("home-secret")),
            };
            i.project_env = FileInput::Read {
                path: "/synthetic/cwd/.octocode/.env".into(),
                text: format!("{key}={}", value("project-secret")),
            };
            let out = resolve_config(&i);
            assert_eq!(
                out.env_value(key),
                Some(expected.as_str()),
                "{key}, explicit={explicit:?}"
            );
            assert_eq!(out.env_value("HOME_ONLY"), Some("home"));
            if ENV_TOKEN_VARS.contains(&key) {
                assert_eq!(
                    out.token
                        .as_ref()
                        .map(super::super::types::PrivateTokenSelection::token),
                    Some(expected.as_str())
                );
            }
            if ENV_TOKEN_VARS.contains(&key) || key == "OCTOCODE_CLASSIFICATION_API" {
                assert!(!format!("{out:?}").contains("-secret"));
            }
            let view = inspector_data(&i, &out);
            assert!(!serde_json::to_string(&view).unwrap().contains("-secret"));
        }
    }
}

#[test]
fn every_config_binding_loads_workspace_before_home_without_lsp_trust() {
    for field in CONFIG_FIELDS {
        if let Some(binding) = field.env {
            let key = binding.name;
            let value = |label: &str| valid_value(key, label);
            for explicit in [false, true] {
                let mut i = input(BTreeMap::new(), None);
                if explicit {
                    i.env.insert(key.into(), value("process-value"));
                }
                i.global_env = FileInput::Read {
                    path: "/home/.env".into(),
                    text: format!("{key}={}\nHOME_ONLY=home", value("home-value")),
                };
                i.project_env = FileInput::Read {
                    path: "/workspace/.octocode/.env".into(),
                    text: format!("{key}={}", value("workspace-value")),
                };
                let out = resolve_config(&i);
                assert_eq!(
                    out.env_value(key),
                    Some(
                        value(if explicit {
                            "process-value"
                        } else if HOME_TRUSTED_ENV_KEYS.contains(&key) {
                            "home-value"
                        } else {
                            "workspace-value"
                        })
                        .as_str()
                    ),
                    "{key}"
                );
                assert_eq!(out.env_value("HOME_ONLY"), Some("home"));
                i.env.clear();
                i.project_env = FileInput::Read {
                    path: "/workspace/.octocode/.env".into(),
                    text: format!("{key}=   "),
                };
                assert_eq!(
                    resolve_config(&i).env_value(key),
                    Some(value("home-value").as_str()),
                    "blank workspace {key}"
                );
            }
        }
    }
}

#[test]
fn token_variables_obey_source_precedence_before_token_priority() {
    // The GitHub token variables are the one group of variables that
    // select one value; every setting has a single variable.
    {
        let group = ENV_TOKEN_VARS;
        for higher_alias in group {
            for lower_alias in group {
                for process in [false, true] {
                    let mut i = input(BTreeMap::new(), None);
                    if process {
                        i.env
                            .insert((*higher_alias).into(), "process-secret".into());
                    }
                    i.global_env = FileInput::Read {
                        path: "/home/.env".into(),
                        text: format!("{lower_alias}=home-secret"),
                    };
                    let project_alias = if process { lower_alias } else { higher_alias };
                    i.project_env = FileInput::Read {
                        path: "/workspace/.octocode/.env".into(),
                        text: format!("{project_alias}=workspace-secret"),
                    };
                    let out = resolve_config(&i);
                    let selected = group
                        .iter()
                        .filter_map(|key| out.env_value(key))
                        .find(|value| !value.trim().is_empty());
                    assert_eq!(
                        selected,
                        Some(if process {
                            "process-secret"
                        } else {
                            "workspace-secret"
                        }),
                        "{higher_alias} vs {lower_alias}"
                    );
                    assert_eq!(
                        out.token
                            .as_ref()
                            .map(super::super::types::PrivateTokenSelection::token),
                        selected
                    );
                    assert!(!format!("{out:?}").contains("-secret"));
                }
            }
        }
    }
}

#[test]
fn retired_classification_alias_is_not_a_credential() {
    let mut i = input(BTreeMap::new(), None);
    i.project_env = FileInput::Read {
        path: "/workspace/.octocode/.env".into(),
        text: "OCTOCODE_JEV_KEY=workspace-secret".into(),
    };
    let out = resolve_config(&i);
    assert_eq!(out.env_value("OCTOCODE_CLASSIFICATION_API"), None);
}

#[test]
fn whitespace_token_values_fall_back_without_leaking() {
    for key in [
        "GH_TOKEN",
        "TAVILY_API_KEY",
        "SERPER_API_KEY",
        "EXA_API_KEY",
        "CUSTOM_SERVICE_TOKEN",
    ] {
        let mut i = input(BTreeMap::from([(key.into(), "   ".into())]), None);
        i.global_env = FileInput::Read {
            path: "/home/.env".into(),
            text: format!("{key}=home-secret"),
        };
        i.project_env = FileInput::Read {
            path: "/workspace/.octocode/.env".into(),
            text: format!("{key}=   "),
        };
        let out = resolve_config(&i);
        assert_eq!(out.env_value(key), Some("home-secret"));
        assert!(!format!("{out:?}").contains("home-secret"));
    }
}

#[test]
fn trusted_dotenv_preserves_classification_opt_out() {
    let mut i = input(
        BTreeMap::from([("OCTOCODE_CLASSIFICATION_API".into(), String::new())]),
        Some(r#"{"classification":{"api":"file-secret"}}"#),
    );
    i.global_env = FileInput::Read {
        path: "/synthetic/home/.env".into(),
        text: "OCTOCODE_CLASSIFICATION_API=home-secret".into(),
    };
    i.project_env = FileInput::Read {
        path: "/synthetic/cwd/.octocode/.env".into(),
        text: "OCTOCODE_CLASSIFICATION_API=project-secret".into(),
    };
    assert_eq!(
        resolve_config(&i).env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("")
    );
}

#[test]
fn unreadable_config_is_invalid_with_a_stable_diagnostic() {
    let mut i = input(BTreeMap::new(), None);
    i.config_file = FileInput::Unreadable {
        path: "/synthetic/home/.octocoderc".into(),
        kind: "permission denied".into(),
    };
    let out = resolve_config(&i);
    assert_eq!(out.source, ConfigSource::Invalid);
    assert_eq!(out.config_path, Some("/synthetic/home/.octocoderc".into()));
    assert_eq!(out.diagnostics[0].code, "config_load_error");
    assert!(out.diagnostics[0].message.contains("permission denied"));
}
