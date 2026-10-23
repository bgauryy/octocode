use super::*;

#[test]
fn builtin_rust_route_runs_rust_analyzer_headless_by_default() {
    let config =
        default_server_for_file("src/lib.rs", "/workspace", &LspDiscoveryOptions::default())
            .expect("rust route");
    let options = config
        .initialization_options
        .expect("headless rust-analyzer options");
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(false))
    );
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
    assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
    assert_eq!(options.pointer("/cachePriming/enable"), Some(&json!(false)));
    assert_eq!(options.pointer("/cargo/targetDir"), Some(&json!(true)));
}

#[test]
fn user_rust_analyzer_options_win_over_headless_defaults() {
    let root = TmpDir::new();
    std::fs::create_dir_all(&root).expect("root");
    let config_path = root.join("lsp.json");
    std::fs::write(
        &config_path,
        r#"{"languageServers":{".rs":{"command":"rust-analyzer","languageId":"rust","initializationOptions":{"procMacro":{"enable":true},"cargo":{"features":"all"}}}}}"#,
    )
    .expect("config");
    let config = default_server_for_file(
        "src/lib.rs",
        &root.to_string_lossy(),
        &LspDiscoveryOptions {
            config_path: Some(config_path),
            ..LspDiscoveryOptions::default()
        },
    )
    .expect("user rust route");
    let options = config.initialization_options.expect("options");
    // User keys win…
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(true)));
    assert_eq!(options.pointer("/cargo/features"), Some(&json!("all")));
    // …and the defaults fill in everything the user left out.
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(false))
    );
    assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
}

#[test]
fn server_defaults_touch_only_rust_analyzer_and_are_idempotent() {
    let mut config = crate::lsp::types::LspLanguageServerConfig {
        command: "/opt/bin/gopls".to_owned(),
        args: None,
        workspace_root: "/w".to_owned(),
        language_id: Some("go".to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    crate::lsp::config::apply_server_default_options(&mut config, None);
    assert!(config.initialization_options.is_none());

    config.command = "C:/tools/Rust-Analyzer.exe".to_owned();
    config.initialization_options = Some(json!({"checkOnSave": true}));
    crate::lsp::config::apply_server_default_options(&mut config, None);
    let once = config.initialization_options.clone();
    crate::lsp::config::apply_server_default_options(&mut config, None);
    assert_eq!(config.initialization_options, once);
    let options = once.expect("options");
    assert_eq!(options.pointer("/checkOnSave"), Some(&json!(true)));
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
}

/// clangd, jdtls, and Metals start headless and read-only by
/// default, on the built-in route and through `apply_server_default_options`;
/// user args and options still win.
#[test]
fn clangd_jdtls_and_metals_routes_start_headless() {
    let options = LspDiscoveryOptions {
        octocode_home: Some(PathBuf::from("/home/fixture/.octocode")),
        ..LspDiscoveryOptions::default()
    };
    let clangd =
        default_server_for_file("src/main.cpp", "/workspace", &options).expect("clangd route");
    let args = clangd.args.expect("clangd args");
    for flag in crate::lsp::config::defaults::CLANGD_HEADLESS_ARGS {
        assert!(args.iter().any(|arg| arg == flag), "{flag} in {args:?}");
    }

    let jdtls =
        default_server_for_file("src/Main.java", "/workspace/java", &options).expect("jdtls route");
    let args = jdtls.args.expect("jdtls args");
    let data = args
        .iter()
        .position(|arg| arg == "-data")
        .and_then(|index| args.get(index + 1))
        .expect("-data <dir>");
    assert!(
        !std::path::Path::new(data).starts_with("/workspace"),
        "jdtls data stays outside the repository: {data}"
    );
    let home = options.octocode_home.clone().expect("octocode home");
    assert_eq!(
        PathBuf::from(data),
        crate::lsp::config::defaults::jdtls_data_dir(&home, "/workspace/java")
    );
    assert_ne!(
        crate::lsp::config::defaults::jdtls_data_dir(&home, "/workspace/java"),
        crate::lsp::config::defaults::jdtls_data_dir(&home, "/workspace/other"),
        "one data dir per workspace"
    );
    let jdtls_options = jdtls.initialization_options.expect("jdtls options");
    assert_eq!(
        jdtls_options.pointer("/settings/java/autobuild/enabled"),
        Some(&json!(false))
    );

    let metals =
        default_server_for_file("build.sbt", "/workspace", &options).expect("metals route");
    let metals_options = metals.initialization_options.expect("metals options");
    assert_eq!(metals_options["isHttpEnabled"], json!(false));
    assert_eq!(metals_options["statusBarProvider"], json!("off"));

    // User values win; defaults fill the rest; re-applying is a no-op.
    let mut config = crate::lsp::types::LspLanguageServerConfig {
        command: "/usr/bin/clangd-18".to_owned(),
        args: Some(vec![
            "--background-index".to_owned(),
            "--log=verbose".to_owned(),
        ]),
        workspace_root: "/w".to_owned(),
        language_id: Some("cpp".to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    crate::lsp::config::apply_server_default_options(&mut config, None);
    crate::lsp::config::apply_server_default_options(&mut config, None);
    assert_eq!(
        config.args.as_deref().unwrap_or_default(),
        [
            "--background-index",
            "--log=verbose",
            "--clang-tidy=false",
            "--pch-storage=memory"
        ]
    );
    config.command = "jdtls".to_owned();
    config.args = Some(vec!["-data".to_owned(), "/mine".to_owned()]);
    config.initialization_options =
        Some(json!({"settings": {"java": {"autobuild": {"enabled": true}}}}));
    crate::lsp::config::apply_server_default_options(&mut config, None);
    assert_eq!(
        config.args.as_deref().unwrap_or_default(),
        ["-data", "/mine"]
    );
    let merged = config.initialization_options.clone().expect("options");
    assert_eq!(
        merged.pointer("/settings/java/autobuild/enabled"),
        Some(&json!(true))
    );
    assert_eq!(
        merged.pointer("/settings/java/import/generatesMetadataFilesAtProjectRoot"),
        Some(&json!(false))
    );
}

/// A built-in server takes the host-derived memory cap; a per-server
/// `maxMemoryMb` in `lsp-servers.json` (0 disables it) is the one
/// override, and `env` of a user entry reaches the server process.
#[test]
fn the_memory_cap_is_host_derived_with_a_per_server_override() {
    let home = TmpDir::new();
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::write(
        home.join("lsp-servers.json"),
        r#"{"languageServers":{".php":{"command":"home-php-server","args":[],"languageId":"php","maxMemoryMb":1500,"env":{"GOMEMLIMIT":"1GiB"}},".lua":{"command":"home-lua-server","args":[],"languageId":"lua","maxMemoryMb":0}}}"#,
    )
    .expect("write user lsp config");
    let options = LspDiscoveryOptions {
        octocode_home: Some(home.to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    let unset = default_server_for_file("a.go", "/workspace", &options).expect("go");
    assert_eq!(unset.max_memory_mb, None, "unset: the host-derived default");
    let user = default_server_for_file("a.php", "/workspace", &options).expect("php");
    assert_eq!(user.max_memory_mb, Some(1500), "per-server wins");
    // A user entry reports its config file, not where its program lives.
    assert_eq!(user.source, ServerSource::User);
    assert_eq!(
        user.env
            .as_ref()
            .and_then(|env| env.get("GOMEMLIMIT"))
            .map(String::as_str),
        Some("1GiB")
    );
    let disabled = default_server_for_file("a.lua", "/workspace", &options).expect("lua");
    assert_eq!(disabled.max_memory_mb, Some(0), "0 disables the cap");
}

/// typescript-language-server runs headless: no network typings, one
/// process, a V8 heap that stays under the cap, and no package.json
/// import scan. Options the user set win.
#[test]
fn typescript_server_gets_headless_options_sized_to_the_cap() {
    let capped = |max_memory_mb: Option<u32>| {
        let mut config = crate::lsp::types::LspLanguageServerConfig {
            command: "typescript-language-server".to_owned(),
            args: None,
            workspace_root: "/workspace".to_owned(),
            language_id: Some("typescript".to_owned()),
            initialization_options: None,
            env: None,
            max_memory_mb,
            source: Default::default(),
        };
        crate::lsp::config::apply_server_default_options(&mut config, None);
        config
    };
    let ts = crate::lsp::config::catalog::default_server_for_file_from(
        "a.ts",
        "/workspace",
        &LspDiscoveryOptions::default(),
        None,
    )
    .expect("ts");
    assert!(ts.initialization_options.is_some(), "headless defaults");
    let init = capped(Some(8192))
        .initialization_options
        .expect("init options");
    assert_eq!(init["disableAutomaticTypingAcquisition"], true, "{init}");
    assert_eq!(init["tsserver"]["useSyntaxServer"], "never", "{init}");
    assert_eq!(init["maxTsServerMemory"], 8192 - 512, "{init}");
    assert_eq!(
        init["preferences"]["includePackageJsonAutoImports"], "off",
        "{init}"
    );
    // No cap, no heap bound.
    assert!(capped(Some(0)).initialization_options.expect("init")["maxTsServerMemory"].is_null());
    // A user value wins.
    let mut config = crate::lsp::types::LspLanguageServerConfig {
        command: "typescript-language-server".to_owned(),
        args: None,
        workspace_root: "/workspace".to_owned(),
        language_id: Some("typescript".to_owned()),
        initialization_options: Some(json!({"maxTsServerMemory": 1234})),
        env: None,
        max_memory_mb: Some(8192),
        source: Default::default(),
    };
    crate::lsp::config::apply_server_default_options(&mut config, None);
    let init = config.initialization_options.expect("init");
    assert_eq!(init["maxTsServerMemory"], 1234, "{init}");
}

/// The default cap follows host memory: a quarter of it, within
/// 4 GiB..8 GiB; an unreadable host keeps the floor.
#[test]
fn the_default_cap_is_a_quarter_of_host_memory_within_bounds() {
    use crate::lsp::spawn_limits::default_memory_cap_mb;
    assert_eq!(default_memory_cap_mb(None), 4_096);
    assert_eq!(default_memory_cap_mb(Some(8_192)), 4_096);
    assert_eq!(default_memory_cap_mb(Some(20_000)), 5_000);
    assert_eq!(default_memory_cap_mb(Some(32_768)), 8_192);
    assert_eq!(default_memory_cap_mb(Some(262_144)), 8_192);
}
