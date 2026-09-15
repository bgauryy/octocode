//! Deterministic native Octocode configuration.
mod acquire;
mod dotenv;
mod loader;
mod resolver;
mod types;
mod validation;
pub use acquire::{acquire_config_input, octocode_home, read_file};
pub use dotenv::{
    apply_env, merged_env, parse_boolean_env, parse_env, parse_int_env, parse_string_array_env,
};
pub use loader::load_config;
pub use resolver::{
    get_config_value, inspector_data, is_persistent_storage_enabled,
    is_persistent_storage_enabled_for_extension, is_stats_enabled, resolve_config,
    resolve_env_token, resolve_sections,
};
pub use types::*;
pub use validation::validate_config;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "octocode-native-config-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("test fixture operation should succeed")
                    .as_nanos()
            ));
            fs::create_dir_all(&p).expect("test fixture operation should succeed");
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
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
            runtime_surface: RuntimeSurface::Mcp,
            revision: 1,
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
            ("ENABLE_AST_REWRITE_APPLY".into(), "true".into()),
            ("REQUEST_TIMEOUT".into(), "12ms".into()),
            ("OCTOCODE_EXTENSION_STORAGE_MODE".into(), "memory".into()),
        ]);
        let out = resolve_config(&input(env, None));
        assert!(out.resolved.local.enable_ast_rewrite_apply);
        assert_eq!(out.resolved.network.timeout, 5000.);
        assert_eq!(out.resolved.extension.storage.mode, "memory");
        assert_eq!(
            out.source,
            ConfigSource::Env,
            "extension storage is in the frozen source key list"
        );
        let only = BTreeMap::from([("ENABLE_AST_REWRITE_APPLY".into(), "true".into())]);
        assert_eq!(
            resolve_config(&input(only, None)).source,
            ConfigSource::Env,
            "ENABLE_AST_REWRITE_APPLY is a frozen source key"
        )
    }
    #[test]
    fn invalid_file_is_all_or_nothing_and_diagnostic() {
        let out = resolve_config(&input(
            BTreeMap::new(),
            Some("{\"local\":{\"enabled\":\"no\"},\"network\":{\"timeout\":5000}}"),
        ));
        assert_eq!(out.source, ConfigSource::Invalid);
        assert!(out.resolved.local.enabled);
        assert_eq!(out.resolved.network.timeout, 30000.);
        assert_eq!(out.diagnostics.len(), 1)
    }
    #[test]
    fn dotenv_protection_and_trust() {
        let (mut map, sources) = merged_env(
            Some("A=g\nPATH=/bad\nOCTOCODE_TOKEN=file"),
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
        assert_eq!(r.skipped_protected, vec!["OCTOCODE_TOKEN", "PATH"]);
        map.clear()
    }
    #[test]
    fn token_is_redacted_and_storage_stats_are_explicit() {
        let env = BTreeMap::from([
            ("OCTOCODE_TOKEN".into(), " synthetic ".into()),
            ("GH_TOKEN".into(), "lower".into()),
        ]);
        let token = resolve_env_token(&env).expect("test fixture operation should succeed");
        assert_eq!(token.token(), "synthetic");
        assert!(!format!("{token:?}").contains("synthetic"));
        let cfg = resolve_sections(
            Some(
                &json!({"storage":{"mode":"memory"},"extension":{"storage":{"mode":"persistent"}}}),
            ),
            &BTreeMap::from([("OCTOCODE_ENABLE_STATS".into(), "true".into())]),
        );
        assert!(!is_stats_enabled(&cfg));
        assert!(is_persistent_storage_enabled_for_extension(&cfg))
    }
    #[test]
    fn fresh_acquisition_observes_file_changes_and_cleans_up() {
        let root = TempDir::new();
        let home = root.0.join("home");
        let cwd = root.0.join("cwd");
        fs::create_dir_all(&home).expect("test fixture operation should succeed");
        fs::create_dir_all(cwd.join(".octocode")).expect("test fixture operation should succeed");
        let env = BTreeMap::from([("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned())]);
        fs::write(home.join(".octocoderc"), "{\"network\":{\"timeout\":5000}}")
            .expect("test fixture operation should succeed");
        let a = resolve_config(&acquire_config_input(
            env.clone(),
            cwd.clone(),
            root.0.clone(),
            false,
            RuntimeSurface::Cli,
            1,
        ));
        fs::write(home.join(".octocoderc"), "{\"network\":{\"timeout\":6000}}")
            .expect("test fixture operation should succeed");
        let b = resolve_config(&acquire_config_input(
            env,
            cwd,
            root.0.clone(),
            false,
            RuntimeSurface::Cli,
            2,
        ));
        assert_eq!(
            (
                a.resolved.network.timeout,
                b.resolved.network.timeout,
                a.revision,
                b.revision
            ),
            (5000., 6000., 1, 2)
        )
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
