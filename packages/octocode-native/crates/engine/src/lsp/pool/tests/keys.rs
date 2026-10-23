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
    // Every built-in server's language waits, objective-c (clangd) and
    // scala (Metals) included.
    assert_eq!(readiness_timeout(Some("objective-c")), Some(20_000));
    assert_eq!(readiness_timeout(Some("scala")), Some(120_000));
    // An unknown/unlisted language still opts out of the readiness wait.
    assert_eq!(readiness_timeout(Some("plaintext")), None);
    assert_eq!(readiness_timeout(None), None);
}

#[test]
fn readiness_maximum_is_the_largest_configured_wait() {
    let longest = [
        "go",
        "rust",
        "java",
        "csharp",
        "swift",
        "shellscript",
        "typescript",
        "typescriptreact",
        "javascript",
        "javascriptreact",
        "python",
        "c",
        "cpp",
        "cuda",
    ]
    .into_iter()
    .filter_map(|language| readiness_timeout(Some(language)))
    .max();
    assert_eq!(longest.map(u64::from), Some(MAX_READINESS_TIMEOUT_MS));
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

#[cfg(unix)]
#[test]
fn canonical_key_follows_workspace_symlinks() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let base = std::env::temp_dir().join(format!(
        "octocode-pool-symlink-{}-{nanos}",
        std::process::id()
    ));
    let real = base.join("real");
    let link = base.join("link");
    std::fs::create_dir_all(&real).expect("create real root");
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");
    let config = |root: &Path| LspLanguageServerConfig {
        command: "server".into(),
        args: None,
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("rust".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let direct = canonical_lsp_key(&config(&real)).expect("direct key");
    let via_link = canonical_lsp_key(&config(&link)).expect("symlink key");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(
        direct, via_link,
        "a symlinked root must share the pool entry"
    );
}
