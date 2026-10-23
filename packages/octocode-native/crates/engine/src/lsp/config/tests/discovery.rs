use super::*;

#[test]
fn workspace_lsp_config_is_ignored_by_default() {
    let root = TmpDir::new();
    write_lsp_config(
        &root,
        r#"{"languageServers":{".ts":{"command":"node","args":["-e","process.exit(99)"],"languageId":"typescript"}}}"#,
    );

    let Some(root_str) = root.to_str() else {
        panic!("temporary root is not utf-8");
    };
    let config = default_server_for_file("demo.ts", root_str, &LspDiscoveryOptions::default())
        .expect("default ts server config");

    assert_ne!(config.command, "node");
    assert_ne!(
        config.args.as_deref(),
        Some(&["-e".to_owned(), "process.exit(99)".to_owned()][..])
    );
}

#[test]
fn trusted_workspace_lsp_config_rejects_interpreter_eval_launch() {
    let root = TmpDir::new();
    write_lsp_config(
        &root,
        r#"{"languageServers":{".ts":{"command":"node","args":["-e","process.exit(99)"],"languageId":"typescript"}}}"#,
    );
    let options = LspDiscoveryOptions {
        trust_project_config: true,
        ..LspDiscoveryOptions::default()
    };

    let Some(root_str) = root.to_str() else {
        panic!("temporary root is not utf-8");
    };
    let config =
        default_server_for_file("demo.ts", root_str, &options).expect("default ts server config");

    assert_ne!(config.command, "node");
    assert_ne!(
        config.args.as_deref(),
        Some(&["-e".to_owned(), "process.exit(99)".to_owned()][..])
    );
}

#[test]
fn explicit_discovery_options_support_arbitrary_extensions() {
    let root = TmpDir::new();
    std::fs::create_dir_all(&root).expect("create temporary root");
    let config_path = root.join("native-lsp.json");
    std::fs::write(
        &config_path,
        r#"{"languageServers":{".php":{"command":"custom-php-server","args":["--stdio"],"languageId":"php"}}}"#,
    )
    .expect("write explicit lsp config");
    let root_str = root.to_string_lossy().into_owned();
    let config = default_server_for_file(
        "demo.php",
        &root_str,
        &LspDiscoveryOptions {
            config_path: Some(config_path),
            ..LspDiscoveryOptions::default()
        },
    )
    .expect("explicit server config");
    assert_eq!(config.command, "custom-php-server");
    assert_eq!(config.language_id.as_deref(), Some("php"));
}

/// Settings come from the host's resolved configuration (which includes
/// the home `.env`), not the process env, and the user server file lives
/// in the octocode home.
#[test]
fn resolved_settings_and_octocode_home_drive_discovery() {
    let home = TmpDir::new();
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::write(
        home.join("lsp-servers.json"),
        r#"{"languageServers":{".php":{"command":"home-php-server","args":[],"languageId":"php"},".ts":{"command":"tsgo","args":["--lsp","-stdio"],"languageId":"typescript"}}}"#,
    )
    .expect("write user lsp config");
    let options = LspDiscoveryOptions {
        octocode_home: Some(home.to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    // A user entry replaces a built-in server (the retired
    // `OCTOCODE_TS_SERVER_PATH` route): here `tsgo` with its own args.
    let ts = default_server_for_file("demo.ts", "/workspace", &options).expect("ts server");
    assert_eq!(ts.command, "tsgo");
    assert_eq!(
        ts.args.as_deref(),
        Some(&["--lsp".to_owned(), "-stdio".to_owned()][..])
    );
    let php = default_server_for_file("demo.php", "/workspace", &options)
        .expect("user server from the octocode home");
    assert_eq!(php.command, "home-php-server");
}

#[test]
fn retired_server_path_env_does_not_override_the_trusted_workspace_config() {
    let root = TmpDir::new();
    write_lsp_config(
        &root,
        r#"{"languageServers":{".ts":{"command":"custom-language-server","args":["--stdio"],"languageId":"typescript"}}}"#,
    );
    let options = LspDiscoveryOptions {
        trust_project_config: true,
        env: BTreeMap::from([(
            "OCTOCODE_TS_SERVER_PATH".to_owned(),
            "env-ts-server".to_owned(),
        )]),
        ..LspDiscoveryOptions::default()
    };

    let Some(root_str) = root.to_str() else {
        panic!("temporary root is not utf-8");
    };
    let config =
        default_server_for_file("demo.ts", root_str, &options).expect("default ts server config");

    assert_eq!(config.command, "custom-language-server");
}
