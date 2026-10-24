use super::*;

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
    );
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
    assert!(warnings.contains(&"Unknown configuration key: extra".into()));
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
    assert!(
        out.diagnostics.iter().any(
            |d| d.code == "invalid_env_value" && d.field_path.as_deref() == Some("lsp.prewarm")
        )
    );
    assert_eq!(out.source, ConfigSource::Env);
    let only = BTreeMap::from([("OCTOCODE_BETA".into(), "true".into())]);
    assert_eq!(
        resolve_config(&input(only, None)).source,
        ConfigSource::Env,
        "OCTOCODE_BETA is a frozen source key"
    );
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
    assert!(!is_stats_enabled(&cfg.unwrap_or_default()));
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
    );
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
    );
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
    assert_eq!(resolve_config(&input(env, None)).source, ConfigSource::Env);
}
