use super::*;

#[test]
fn readiness_timeout_gates_the_slow_indexing_languages() {
    // Languages whose servers index before answering must get a readiness
    // wait so a query cannot race indexing and return a partial/empty answer
    // without the partiality signal.
    for language in [
        "go",
        "rust",
        "java",
        "csharp",
        "swift",
        "shellscript",
        "typescript",
        "typescriptreact",
        "javascript",
        "python",
        "c",
        "cpp",
        "cuda",
    ] {
        assert!(
            readiness_timeout(Some(language)).is_some(),
            "{language} must gate on indexing readiness"
        );
    }
    // Preserve existing budgets for the already-wired set.
    assert_eq!(readiness_timeout(Some("go")), Some(15_000));
    assert_eq!(readiness_timeout(Some("rust")), Some(60_000));
    assert_eq!(readiness_timeout(Some("java")), Some(120_000));
    assert_eq!(readiness_timeout(Some("csharp")), Some(30_000));
    assert_eq!(readiness_timeout(Some("swift")), Some(30_000));
    assert_eq!(readiness_timeout(Some("shellscript")), Some(2_000));
    // Newly-gated languages.
    assert_eq!(readiness_timeout(Some("typescript")), Some(30_000));
    assert_eq!(readiness_timeout(Some("typescriptreact")), Some(30_000));
    assert_eq!(readiness_timeout(Some("javascript")), Some(30_000));
    assert_eq!(readiness_timeout(Some("python")), Some(15_000));
    assert_eq!(readiness_timeout(Some("c")), Some(20_000));
    assert_eq!(readiness_timeout(Some("cpp")), Some(20_000));
    assert_eq!(readiness_timeout(Some("cuda")), Some(20_000));
    // Every built-in server's language waits, scala (Metals) included.
    assert_eq!(readiness_timeout(Some("scala")), Some(120_000));
    // An unknown/unlisted language still opts out of the readiness wait.
    assert_eq!(readiness_timeout(Some("plaintext")), None);
    assert_eq!(readiness_timeout(None), None);
}

#[test]
fn canonical_key_normalizes_root_and_all_effective_config() {
    let root = std::env::current_dir().expect("cwd");
    let mut env_a = HashMap::new();
    env_a.insert("B".into(), "2".into());
    env_a.insert("A".into(), "1".into());
    let mut env_b = HashMap::new();
    env_b.insert("A".into(), "1".into());
    env_b.insert("B".into(), "2".into());
    let first = LspLanguageServerConfig {
        command: "server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: root.join("x/../workspace").to_string_lossy().into_owned(),
        language_id: Some("rust".into()),
        document_language_ids: Default::default(),
        initialization_options: Some(serde_json::json!({"z": 1, "nested": {"b": 2, "a": 1}})),
        env: Some(env_a),
        max_memory_mb: None,
        source: Default::default(),
    };
    let second = LspLanguageServerConfig {
        workspace_root: root.join("workspace").to_string_lossy().into_owned(),
        initialization_options: Some(serde_json::json!({"nested": {"a": 1, "b": 2}, "z": 1})),
        env: Some(env_b),
        max_memory_mb: None,
        ..first.clone()
    };
    assert_eq!(
        canonical_lsp_key(&first).expect("key"),
        canonical_lsp_key(&second).expect("key")
    );
    let mut changed = second;
    changed.args = Some(vec!["--socket".into()]);
    assert_ne!(
        canonical_lsp_key(&first).expect("key"),
        canonical_lsp_key(&changed).expect("changed key")
    );
    let mut capped = first.clone();
    capped.max_memory_mb = Some(1_024);
    assert_ne!(
        canonical_lsp_key(&first).expect("key"),
        canonical_lsp_key(&capped).expect("capped key"),
        "the memory cap is effective config; differently-capped servers must not share a pool entry"
    );
}

#[test]
fn canonical_key_separates_custom_document_language_ids() {
    let root = tempfile::tempdir().expect("root");
    let config = LspLanguageServerConfig {
        command: "custom-server".to_owned(),
        args: None,
        workspace_root: root.path().to_string_lossy().into_owned(),
        language_id: Some("cpp".to_owned()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let mut custom = config.clone();
    custom
        .document_language_ids
        .insert(".h".into(), "cpp".into());
    assert_ne!(
        canonical_lsp_key(&config).expect("default key"),
        canonical_lsp_key(&custom).expect("custom key")
    );
    custom.document_language_ids.insert(".h".into(), "c".into());
    let mut alternate = custom.clone();
    alternate
        .document_language_ids
        .insert(".h".into(), "cpp".into());
    assert_ne!(
        canonical_lsp_key(&custom).expect("C header key"),
        canonical_lsp_key(&alternate).expect("C++ header key")
    );
    let mut serialized = serde_json::to_value(&config).expect("config");
    serialized
        .as_object_mut()
        .expect("object")
        .remove("document_language_ids");
    let restored: LspLanguageServerConfig =
        serde_json::from_value(serialized).expect("default IDs");
    assert!(restored.document_language_ids.is_empty());
}

#[cfg(unix)]
#[test]
fn canonical_key_follows_workspace_symlinks() {
    let base = crate::test_support::TmpDir::new();
    let real = base.join("real");
    let link = base.join("link");
    std::fs::create_dir_all(&real).expect("create real root");
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");
    let config = |root: &Path| LspLanguageServerConfig {
        command: "server".into(),
        args: None,
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("rust".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let direct = canonical_lsp_key(&config(&real)).expect("direct key");
    let via_link = canonical_lsp_key(&config(&link)).expect("symlink key");
    assert_eq!(
        direct, via_link,
        "a symlinked root must share the pool entry"
    );
}

#[test]
fn family_key_ignores_options_and_splits_on_workspace_binary_and_language() {
    let root = std::env::current_dir().expect("cwd");
    let first = LspLanguageServerConfig {
        command: "server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("typescript".into()),
        document_language_ids: Default::default(),
        initialization_options: Some(serde_json::json!({"a": 1})),
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let family = |config: &LspLanguageServerConfig| canonical_lsp_family(config).expect("family");
    let reconfigured = LspLanguageServerConfig {
        args: Some(vec!["--socket".into()]),
        initialization_options: Some(serde_json::json!({"a": 2})),
        max_memory_mb: Some(512),
        ..first.clone()
    };
    assert_eq!(family(&first), family(&reconfigured));
    assert_ne!(
        canonical_lsp_key(&first).expect("key"),
        canonical_lsp_key(&reconfigured).expect("key")
    );
    for other in [
        LspLanguageServerConfig {
            command: "other".into(),
            ..first.clone()
        },
        LspLanguageServerConfig {
            workspace_root: root.join("elsewhere").to_string_lossy().into_owned(),
            ..first.clone()
        },
        LspLanguageServerConfig {
            language_id: Some("javascript".into()),
            ..first.clone()
        },
    ] {
        assert_ne!(family(&first), family(&other));
    }
}

#[cfg(unix)]
mod one_server_per_resolution {
    use super::*;
    use crate::lsp::config::identity::fixture::FakeToolchains;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn rust_analyzer(
        tc: &FakeToolchains,
        path: &str,
        extra: &[(&str, &str)],
    ) -> LspLanguageServerConfig {
        let workspace = tc.root.join("work");
        std::fs::create_dir_all(&workspace).expect("workspace");
        LspLanguageServerConfig {
            command: "rust-analyzer".to_owned(),
            args: None,
            workspace_root: workspace.to_string_lossy().into_owned(),
            language_id: Some("rust".to_owned()),
            document_language_ids: Default::default(),
            initialization_options: None,
            env: Some(tc.env(path, extra)),
            max_memory_mb: None,
            source: Default::default(),
        }
    }

    /// Servers the pool starts for `configs`, asked for in turn.
    async fn starts(configs: &[LspLanguageServerConfig]) -> usize {
        let pool = pool(4, 60_000);
        let started = Arc::new(AtomicUsize::new(0));
        for config in configs {
            let started = Arc::clone(&started);
            pool.acquire(
                canonical_lsp_key(config).expect("key"),
                canonical_lsp_family(config).expect("family"),
                || async move {
                    let id = started.fetch_add(1, Ordering::SeqCst);
                    Ok(Some(FakeClient::new(id)))
                },
            )
            .await
            .expect("acquire")
            .expect("client");
        }
        started.load(Ordering::SeqCst)
    }

    /// A GUI host and a terminal that differ in `PATH`, `LANG` and `TMPDIR`
    /// and resolve rust-analyzer and the toolchain alike share one server.
    #[tokio::test]
    async fn clients_differing_only_in_path_lang_and_tmpdir_share_one_server() {
        let tc = FakeToolchains::install();
        let gui = rust_analyzer(
            &tc,
            "{bin}:/usr/bin:/bin",
            &[("LANG", "C"), ("TMPDIR", "/tmp/gui")],
        );
        let terminal = rust_analyzer(
            &tc,
            "/opt/homebrew/bin:/usr/bin:{bin}:/bin",
            &[("LANG", "en_US.UTF-8"), ("TMPDIR", "/var/folders/term")],
        );
        assert_eq!(starts(&[gui.clone(), terminal.clone(), gui]).await, 1);
    }

    /// A client whose environment selects another toolchain gets its own.
    #[tokio::test]
    async fn clients_resolving_to_other_toolchains_do_not_share() {
        let tc = FakeToolchains::install();
        let stable = rust_analyzer(&tc, "{bin}:/usr/bin", &[]);
        let nightly = rust_analyzer(&tc, "{bin}:/usr/bin", &[("RUSTUP_TOOLCHAIN", "nightly")]);
        assert_eq!(starts(&[stable, nightly]).await, 2);
    }
}
