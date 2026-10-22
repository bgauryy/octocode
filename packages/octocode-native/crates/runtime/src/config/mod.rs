//! Deterministic native Octocode configuration.
mod acquire;
mod dotenv;
mod edit;
#[cfg(test)]
mod effect_tests;
mod json_edit;
#[cfg(test)]
mod layering_tests;
mod loader;
mod manage;
pub(crate) mod resolver;
mod types;
mod validation;
pub use acquire::acquire_config_input;
pub(crate) use acquire::octocode_home;
pub use dotenv::parse_env;
pub use edit::{
    backup_path, config_revision, edit_scoped_env, read_private_config, replace_private_config,
    validate_env_edit,
};
pub use json_edit::{edit_config_json, parse_config_json};
pub(crate) use loader::load_config;
pub use manage::{edit_setting, inspect_management};
#[cfg(test)]
pub(crate) use resolver::resolve_sections;
pub(crate) use resolver::{get_config_value, inspector_data, is_stats_enabled, nearest_tool_name};
pub use resolver::{is_persistent_storage_enabled, resolve_config};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::dotenv::{apply_env, merged_env};
    use super::resolver::resolve_env_token;
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    /// A valid value of `key` labeled `label`: an http(s) URL for a URL
    /// setting's variable, `label` itself otherwise.
    fn valid_value(key: &str, label: &str) -> String {
        let url = CONFIG_FIELDS
            .iter()
            .any(|field| field.kind == ConfigFieldKind::Url && field.env_name() == Some(key));
        if url {
            format!("https://{label}.example")
        } else {
            label.to_owned()
        }
    }
    fn input(env: BTreeMap<String, String>, file: Option<&str>) -> ConfigInput {
        let home = std::path::PathBuf::from("/synthetic/home");
        ConfigInput {
            env,
            cwd: "/synthetic/cwd".into(),
            os_home: "/synthetic".into(),
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
    /// The identity digest follows every setting and declared variable,
    /// including the config file's content, and ignores undeclared ones.
    #[test]
    fn identity_digest_tracks_declared_settings_only() {
        let digest = |env: &[(&str, &str)], file: Option<&str>| {
            let env = env
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect();
            resolve_config(&input(env, file)).identity_digest()
        };
        let base = digest(&[], None);
        assert_eq!(digest(&[], None), base);
        assert_eq!(digest(&[("TERM_SESSION_ID", "x")], None), base);
        for name in DECLARED_ENV_KEYS {
            assert_ne!(
                digest(&[(name, &valid_value(name, "x"))], None),
                base,
                "{name}"
            );
        }
        assert_ne!(
            digest(&[], Some("{\"local\":{\"enabled\":false}}")),
            base,
            "a config file edit"
        );
    }

    #[test]
    fn dotenv_edge_behavior() {
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
    fn validation_covers_types_paths_and_warnings() {
        let (ok, _) = validation::config_issues(
            &json!({"lsp":{"prewarm":"off","autoInstall":"auto"},"tools":{"enabled":null}}),
        );
        assert!(ok.is_empty());
        let (mistyped, _) =
            validation::config_issues(&json!({"lsp":{"prewarm":5,"autoInstall":"sometimes"}}));
        assert_eq!(
            mistyped
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec!["lsp.autoInstall", "lsp.prewarm"]
        );
        let (bad, warnings) = validation::config_issues(
            &json!({"local":{"allowedPaths":["relative","/a/../b"]},"extra":1}),
        );
        assert!(!bad.is_empty());
        assert!(warnings.contains(&"Unknown configuration key: extra".into()))
    }
    #[test]
    fn all_fields_and_source_quirk_resolve() {
        let env = BTreeMap::from([
            ("OCTOCODE_BETA".into(), "true".into()),
            ("OCTOCODE_LSP_PREWARM".into(), "sometimes".into()),
        ]);
        let out = resolve_config(&input(env, None));
        assert!(out.resolved.local.beta);
        // `sometimes` is not a prewarm mode: the setting keeps its default and says so.
        assert_eq!(out.resolved.lsp.prewarm, LspPrewarm::Targeted);
        assert!(out.diagnostics.iter().any(
            |d| d.code == "invalid_env_value" && d.field_path.as_deref() == Some("lsp.prewarm")
        ));
        assert_eq!(out.source, ConfigSource::Env);
        let only = BTreeMap::from([("OCTOCODE_BETA".into(), "true".into())]);
        assert_eq!(
            resolve_config(&input(only, None)).source,
            ConfigSource::Env,
            "OCTOCODE_BETA is a frozen source key"
        )
    }
    #[test]
    fn invalid_field_is_dropped_alone_with_a_warning() {
        let out = resolve_config(&input(
            BTreeMap::new(),
            Some("{\"local\":{\"enabled\":\"no\"},\"lsp\":{\"prewarm\":\"off\"}}"),
        ));
        assert_eq!(out.source, ConfigSource::File);
        assert!(out.resolved.local.enabled, "bad field → default");
        assert_eq!(
            out.resolved.lsp.prewarm,
            LspPrewarm::Off,
            "valid sibling kept"
        );
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].severity, Severity::Warning);
        assert_eq!(
            out.diagnostics[0].field_path.as_deref(),
            Some("local.enabled")
        );
    }
    #[test]
    fn dotenv_protection_and_trust() {
        let merged = merged_env(Some("A=g\nPATH=/bad\nGITHUB_TOKEN=file"), Some("A=p\nP=x"));
        let mut target = BTreeMap::from([("A".into(), "host".into()), ("P".into(), String::new())]);
        let r = apply_env(merged, &mut target);
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
        fs::write(home.join(".octocoderc"), "{\"lsp\":{\"prewarm\":\"off\"}}")
            .expect("test fixture operation should succeed");
        let a = resolve_config(&acquire_config_input(
            env.clone(),
            cwd.clone(),
            root.path().to_path_buf(),
            RuntimeSurface::Cli,
        ));
        fs::write(home.join(".octocoderc"), "{\"lsp\":{\"prewarm\":\"all\"}}")
            .expect("test fixture operation should succeed");
        let b = resolve_config(&acquire_config_input(
            env,
            cwd,
            root.path().to_path_buf(),
            RuntimeSurface::Cli,
        ));
        assert_eq!(
            (a.resolved.lsp.prewarm, b.resolved.lsp.prewarm),
            (LspPrewarm::Off, LspPrewarm::All)
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
        let file = "{\"lsp\":{\"prewarm\":\"off\"}}";
        assert_eq!(
            resolve_config(&input(BTreeMap::new(), None)).source,
            ConfigSource::Defaults
        );
        assert_eq!(
            resolve_config(&input(BTreeMap::new(), Some(file))).source,
            ConfigSource::File
        );
        let env = BTreeMap::from([("OCTOCODE_LSP_PREWARM".into(), "all".into())]);
        let mixed = resolve_config(&input(env.clone(), Some(file)));
        assert_eq!(mixed.source, ConfigSource::Mixed);
        assert_eq!(
            get_config_value(&mixed.resolved, "lsp.prewarm"),
            Some(json!("all"))
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
                let expected =
                    if expected == "project-secret" && HOME_TRUSTED_ENV_KEYS.contains(&key) {
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
                        out.token.as_ref().map(|token| token.token()),
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
                        assert_eq!(out.token.as_ref().map(|token| token.token()), selected);
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
}
