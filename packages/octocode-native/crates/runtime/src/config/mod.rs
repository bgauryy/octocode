//! Deterministic native Octocode configuration.
mod acquire;
mod dotenv;
mod edit;
mod json_edit;
#[cfg(test)]
mod layering_tests;
mod loader;
mod manage;
pub(crate) mod resolver;
mod types;
mod validation;
pub use acquire::{acquire_config_input, octocode_home};
pub use dotenv::{
    apply_env, merged_env, parse_boolean_env, parse_env, parse_int_env, parse_string_array_env,
};
pub use edit::{
    backup_path, config_revision, edit_scoped_env, read_private_config, replace_private_config,
    validate_env_edit,
};
pub use json_edit::{edit_config_json, parse_config_json};
pub use loader::load_config;
pub use manage::{edit_setting, inspect_management, validate_env_value};
pub use resolver::{
    get_config_value, inspector_data, is_persistent_storage_enabled, is_stats_enabled,
    resolve_config, resolve_env_token, resolve_sections,
};
pub use types::*;
pub use validation::validate_config;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    fn input(env: BTreeMap<String, String>, file: Option<&str>) -> ConfigInput {
        let home = std::path::PathBuf::from("/synthetic/home");
        ConfigInput {
            env,
            cwd: "/synthetic/cwd".into(),
            os_home: "/synthetic".into(),
            trusted_project: false,
            global_env: FileInput::Missing {
                path: home.join(".env"),
            },
            project_env: FileInput::Missing {
                path: "/synthetic/cwd/.octocode/.env".into(),
            },
            config_file: match file {
                Some(text) => FileInput::Read {
                    path: home.join(".octocoderc"),
                    text: text.into(),
                },
                None => FileInput::Missing {
                    path: home.join(".octocoderc"),
                },
            },
            project_config_file: FileInput::Missing {
                path: "/synthetic/cwd/.octocode/.octocoderc".into(),
            },
            runtime_surface: RuntimeSurface::Mcp,
        }
    }
    #[test]
    fn dotenv_freezes_js_edge_behavior() {
        assert_eq!(
            parse_env(Some(
                " export URL = x?a=1&b=2\r\nA=one\nA=two\nINLINE=v # kept\nLEFT=\"abc"
            )),
            BTreeMap::from([
                ("A".into(), "two".into()),
                ("INLINE".into(), "v # kept".into()),
                ("LEFT".into(), "abc".into()),
                ("URL".into(), "x?a=1&b=2".into())
            ])
        );
        assert_eq!(
            ["12.9", "12ms", "0x10", "1e3", "-7"].map(|s| parse_int_env(Some(s))),
            [Some(12), Some(12), Some(0), Some(1), Some(-7)]
        )
    }
    #[test]
    fn config_parser_is_comment_json_not_full_json5() {
        let p = std::path::PathBuf::from("/synthetic/.octocoderc");
        assert!(
            load_config(&FileInput::Read {
                path: p.clone(),
                text: "{\"x\":\"https://x/*y*/?q=//z\",}".into()
            })
            .success
        );
        assert!(
            !load_config(&FileInput::Read {
                path: p.clone(),
                text: "{'x':1}".into()
            })
            .success
        );
        assert_eq!(
            load_config(&FileInput::Read {
                path: p,
                text: "[]".into()
            })
            .error
            .as_deref(),
            Some("Config file has invalid structure: must be a JSON object")
        )
    }
    #[test]
    fn validation_covers_fractional_ranges_paths_and_warnings() {
        let ok = validate_config(
            &json!({"network":{"timeout":5000.5,"maxRetries":2.5},"tools":{"enabled":null}}),
        );
        assert!(ok.valid);
        let bad =
            validate_config(&json!({"local":{"allowedPaths":["relative","/a/../b"]},"extra":1}));
        assert!(!bad.valid);
        assert!(
            bad.warnings
                .contains(&"Unknown configuration key: extra".into())
        )
    }
    #[test]
    fn all_fields_and_source_quirk_resolve() {
        let env = BTreeMap::from([
            ("OCTOCODE_BETA".into(), "true".into()),
            ("REQUEST_TIMEOUT".into(), "12ms".into()),
        ]);
        let out = resolve_config(&input(env, None));
        assert!(out.resolved.local.beta);
        assert_eq!(out.resolved.network.timeout, 5000.);
        assert_eq!(out.source, ConfigSource::Env);
        let only = BTreeMap::from([("OCTOCODE_BETA".into(), "true".into())]);
        assert_eq!(
            resolve_config(&input(only, None)).source,
            ConfigSource::Env,
            "OCTOCODE_BETA is a frozen source key"
        )
    }
    #[test]
    fn classification_max_concurrency_defaults_overrides_and_clamps() {
        let out = resolve_config(&input(BTreeMap::new(), None));
        assert_eq!(out.resolved.classification.max_concurrency, 10.);
        for (raw, expected) in [("4", 4.), ("0", 1.), ("1000", 64.), ("nope", 10.)] {
            let env =
                BTreeMap::from([("OCTOCODE_CLASSIFICATION_CONCURRENCY".into(), raw.to_owned())]);
            assert_eq!(
                resolve_config(&input(env, None))
                    .resolved
                    .classification
                    .max_concurrency,
                expected,
                "OCTOCODE_CLASSIFICATION_CONCURRENCY={raw}"
            );
        }
        let file = resolve_config(&input(
            BTreeMap::new(),
            Some("{\"classification\":{\"maxConcurrency\":3}}"),
        ));
        assert_eq!(file.resolved.classification.max_concurrency, 3.);
        assert_eq!(
            get_config_value(&file.resolved, "classification.maxConcurrency"),
            Some(json!(3.0))
        );
    }
    #[test]
    fn invalid_field_is_dropped_alone_with_a_warning() {
        let out = resolve_config(&input(
            BTreeMap::new(),
            Some("{\"local\":{\"enabled\":\"no\"},\"network\":{\"timeout\":5000}}"),
        ));
        assert_eq!(out.source, ConfigSource::File);
        assert!(out.resolved.local.enabled, "bad field → default");
        assert_eq!(out.resolved.network.timeout, 5000., "valid sibling kept");
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].severity, Severity::Warning);
        assert_eq!(
            out.diagnostics[0].field_path.as_deref(),
            Some("local.enabled")
        );
    }
    #[test]
    fn dotenv_protection_and_trust() {
        let (mut map, sources) = merged_env(
            Some("A=g\nPATH=/bad\nGITHUB_TOKEN=file"),
            Some("A=p\nP=x"),
            true,
        );
        let mut target = BTreeMap::from([("A".into(), "host".into()), ("P".into(), String::new())]);
        let r = apply_env(&map, sources, &mut target);
        assert_eq!(
            target
                .get("A")
                .expect("test fixture operation should succeed"),
            "host"
        );
        assert_eq!(
            target
                .get("P")
                .expect("test fixture operation should succeed"),
            "x"
        );
        assert_eq!(r.skipped_protected, vec!["PATH"]);
        assert_eq!(target.get("GITHUB_TOKEN").map(String::as_str), Some("file"));
        map.clear()
    }
    #[test]
    fn token_is_redacted_and_storage_stats_are_explicit() {
        let env = BTreeMap::from([
            ("GH_TOKEN".into(), " synthetic ".into()),
            ("GITHUB_TOKEN".into(), "lower".into()),
        ]);
        let token = resolve_env_token(&env).expect("test fixture operation should succeed");
        assert_eq!(token.token(), "synthetic");
        assert!(!format!("{token:?}").contains("synthetic"));
        let cfg = resolve_sections(
            &[&json!({"storage":{"mode":"memory"}})],
            &BTreeMap::from([("OCTOCODE_ENABLE_STATS".into(), "true".into())]),
        );
        assert!(cfg.is_ok());
        assert!(!is_stats_enabled(&cfg.unwrap_or_default()))
    }
    #[test]
    fn fresh_acquisition_observes_file_changes_and_cleans_up() {
        let root = tempfile::tempdir().expect("test fixture operation should succeed");
        let home = root.path().join("home");
        let cwd = root.path().join("cwd");
        fs::create_dir_all(&home).expect("test fixture operation should succeed");
        fs::create_dir_all(cwd.join(".octocode")).expect("test fixture operation should succeed");
        let env = BTreeMap::from([("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned())]);
        fs::write(home.join(".octocoderc"), "{\"network\":{\"timeout\":5000}}")
            .expect("test fixture operation should succeed");
        let a = resolve_config(&acquire_config_input(
            env.clone(),
            cwd.clone(),
            root.path().to_path_buf(),
            false,
            RuntimeSurface::Cli,
        ));
        fs::write(home.join(".octocoderc"), "{\"network\":{\"timeout\":6000}}")
            .expect("test fixture operation should succeed");
        let b = resolve_config(&acquire_config_input(
            env,
            cwd,
            root.path().to_path_buf(),
            false,
            RuntimeSurface::Cli,
        ));
        assert_eq!(
            (a.resolved.network.timeout, b.resolved.network.timeout),
            (5000., 6000.)
        )
    }
    #[test]
    fn oversized_config_layer_is_unreadable_not_truncated() {
        let root = tempfile::tempdir().expect("test fixture operation should succeed");
        let path = root.path().join(".octocoderc");
        fs::write(&path, " ".repeat(4 * 1024 * 1024 + 1)).expect("fixture");
        assert!(matches!(
            acquire::read_file(path),
            FileInput::Unreadable { .. }
        ));
    }
    #[test]
    fn home_resolution_and_file_states_are_portable() {
        let cwd = std::path::Path::new("/work");
        let os = std::path::Path::new("/users/test");
        assert_eq!(
            octocode_home(&BTreeMap::new(), cwd, os),
            std::path::PathBuf::from("/users/test/.octocode")
        );
        assert_eq!(
            octocode_home(
                &BTreeMap::from([("OCTOCODE_HOME".into(), "  relative/ユニコード ".into())]),
                cwd,
                os
            ),
            std::path::PathBuf::from("/work/relative/ユニコード")
        );
        let p = "/synthetic/.octocoderc".into();
        assert_eq!(
            load_config(&FileInput::Missing { path: p })
                .error
                .as_deref(),
            Some("Config file does not exist")
        )
    }
    #[test]
    fn source_labels_and_dot_lookup_match_reference() {
        let file = "{\"network\":{\"timeout\":5000}}";
        assert_eq!(
            resolve_config(&input(BTreeMap::new(), None)).source,
            ConfigSource::Defaults
        );
        assert_eq!(
            resolve_config(&input(BTreeMap::new(), Some(file))).source,
            ConfigSource::File
        );
        let env = BTreeMap::from([("REQUEST_TIMEOUT".into(), "6000".into())]);
        let mixed = resolve_config(&input(env.clone(), Some(file)));
        assert_eq!(mixed.source, ConfigSource::Mixed);
        assert_eq!(
            get_config_value(&mixed.resolved, "network.timeout"),
            Some(json!(6000.0))
        );
        assert_eq!(get_config_value(&mixed.resolved, "does.not.exist"), None);
        assert_eq!(resolve_config(&input(env, None)).source, ConfigSource::Env)
    }
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
        i.trusted_project = true;
        let out = resolve_config(&i);
        let view = inspector_data(&i, &out);
        assert_eq!((view.global_key_count, view.project_key_count), (1, 1));
        assert_eq!(view.storage_mode, "memory");
        assert!(view.is_set("HOST", &out.effective_env));
        let printed = serde_json::to_string(&view).expect("test fixture operation should succeed");
        assert!(!printed.contains("private-"));
        assert!(!format!("{out:?}").contains("private-"));
        assert_eq!(view.config_keys, vec!["local", "storage"])
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
        i.trusted_project = true;
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
        assert!(validate_config(&json!({"classification": {"api": "x"}})).valid);
    }
    #[test]
    fn trusted_dotenv_credentials_use_process_then_project_then_home() {
        for key in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "OCTOCODE_CLASSIFICATION_API",
            "OCTOCODE_CLASSIFICATION_TYPE",
            "OCTOCODE_CLASSIFICATION_API_HOST",
        ] {
            for (trusted, explicit, expected) in [
                (false, None, "project-secret"),
                (true, None, "project-secret"),
                (true, Some("process-secret"), "process-secret"),
            ] {
                // Home-trusted keys (e.g. an endpoint) never come from a workspace.
                let expected =
                    if expected == "project-secret" && HOME_TRUSTED_ENV_KEYS.contains(&key) {
                        "home-secret"
                    } else {
                        expected
                    };
                let env = explicit
                    .map(|value| BTreeMap::from([(key.into(), value.into())]))
                    .unwrap_or_default();
                let mut i = input(env, None);
                i.trusted_project = trusted;
                i.global_env = FileInput::Read {
                    path: "/synthetic/home/.env".into(),
                    text: format!("{key}=home-secret\nHOME_ONLY=home"),
                };
                i.project_env = FileInput::Read {
                    path: "/synthetic/cwd/.octocode/.env".into(),
                    text: format!("{key}=project-secret"),
                };
                let out = resolve_config(&i);
                assert_eq!(
                    out.env_value(key),
                    Some(expected),
                    "{key}, trusted={trusted}"
                );
                assert_eq!(out.env_value("HOME_ONLY"), Some("home"));
                if ENV_TOKEN_VARS.contains(&key) {
                    assert_eq!(
                        out.token.as_ref().map(|token| token.token()),
                        Some(expected)
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
            for binding in field.env {
                let key = binding.name;
                for explicit in [false, true] {
                    let mut i = input(BTreeMap::new(), None);
                    assert!(!i.trusted_project);
                    if explicit {
                        i.env.insert(key.into(), "process-value".into());
                    }
                    i.global_env = FileInput::Read {
                        path: "/home/.env".into(),
                        text: format!("{key}=home-value\nHOME_ONLY=home"),
                    };
                    i.project_env = FileInput::Read {
                        path: "/workspace/.octocode/.env".into(),
                        text: format!("{key}=workspace-value"),
                    };
                    let out = resolve_config(&i);
                    assert_eq!(
                        out.env_value(key),
                        Some(if explicit {
                            "process-value"
                        } else if HOME_TRUSTED_ENV_KEYS.contains(&key) {
                            "home-value"
                        } else {
                            "workspace-value"
                        }),
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
                        Some("home-value"),
                        "blank workspace {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn token_aliases_obey_source_precedence_before_alias_priority() {
        let groups = std::iter::once(ENV_TOKEN_VARS.to_vec()).chain(
            CONFIG_FIELDS
                .iter()
                .filter(|field| field.credential && field.env.len() > 1)
                .map(|field| field.env.iter().map(|binding| binding.name).collect()),
        );
        for group in groups {
            for higher_alias in &group {
                for lower_alias in &group {
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
                        if group == ENV_TOKEN_VARS {
                            assert_eq!(out.token.as_ref().map(|token| token.token()), selected);
                        }
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
        i.trusted_project = true;
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
}
