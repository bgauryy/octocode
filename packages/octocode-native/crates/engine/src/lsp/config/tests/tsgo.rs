use super::*;

/// A trusted workspace's native TypeScript server is preferred over
/// tsserver; an untrusted one never swaps in a checkout's executable.
#[cfg(unix)]
#[test]
fn a_trusted_workspace_tsgo_is_preferred_over_tsserver() {
    use std::os::unix::fs::PermissionsExt;
    let root = TmpDir::new();
    let bin = root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let tsgo = bin.join("tsgo");
    std::fs::write(&tsgo, "#!/bin/sh\n").expect("tsgo");
    std::fs::set_permissions(&tsgo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let Some(root_str) = root.to_str() else {
        panic!("temporary root is not utf-8");
    };
    let trusted = LspDiscoveryOptions {
        trust_project_config: true,
        ..LspDiscoveryOptions::default()
    };
    let config =
        crate::lsp::config::catalog::default_server_for_file_from("a.ts", root_str, &trusted, None)
            .expect("ts");
    assert_eq!(PathBuf::from(&config.command), tsgo);
    assert_eq!(
        config.args.as_deref(),
        Some(&["--lsp".to_owned(), "-stdio".to_owned()][..])
    );
    // tsgo is a Go program: its GC works toward GOMEMLIMIT, set 512 MiB
    // under the memory cap unless the user's `env` already chose one.
    let cap = crate::lsp::spawn_limits::effective_memory_cap_mb(config.max_memory_mb)
        .expect("the default cap is on");
    assert_eq!(
        config.env.as_ref().and_then(|env| env.get("GOMEMLIMIT")),
        Some(&format!("{}MiB", cap - 512)),
        "{config:?}"
    );
    // tsgo drops constructor functions and `this` expandos, so JavaScript
    // stays on tsserver; every TypeScript extension takes the native one.
    for file in ["a.tsx", "a.mts", "a.cts"] {
        let ts = crate::lsp::config::catalog::default_server_for_file_from(
            file, root_str, &trusted, None,
        )
        .expect(file);
        assert_eq!(PathBuf::from(&ts.command), tsgo, "{file}");
    }
    for file in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
        let js = crate::lsp::config::catalog::default_server_for_file_from(
            file, root_str, &trusted, None,
        )
        .expect(file);
        assert_ne!(PathBuf::from(&js.command), tsgo, "{file} stays on tsserver");
    }
    // `None`: an install tree without a bundled tsgo (the checkout that
    // runs these tests ships one beside the test binary); the bundled
    // route has its own tests below.
    let untrusted = crate::lsp::config::catalog::default_server_for_file_from(
        "a.ts",
        root_str,
        &LspDiscoveryOptions::default(),
        None,
    )
    .expect("ts");
    assert_ne!(PathBuf::from(&untrusted.command), tsgo);
    assert!(
        format!("{} {:?}", untrusted.command, untrusted.args)
            .contains("typescript-language-server"),
        "{untrusted:?}"
    );
}

/// An install tree whose `node_modules` ships octocode's own tsgo, plus
/// the anchor file (the executable or addon) inside it.
#[cfg(unix)]
fn install_with_bundled_tsgo() -> (TmpDir, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let install = TmpDir::new();
    let package = install
        .join("node_modules")
        .join("@typescript")
        .join(format!(
            "native-preview-{}",
            crate::lsp::managed::npm_platform_id()
        ))
        .join("lib");
    std::fs::create_dir_all(&package).expect("package dir");
    let tsgo = package.join("tsgo");
    std::fs::write(&tsgo, "#!/bin/sh\n").expect("tsgo");
    std::fs::set_permissions(&tsgo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let anchor = install.join("node_modules/octocode-native/octocode.node");
    std::fs::create_dir_all(anchor.parent().expect("parent")).expect("anchor dir");
    std::fs::write(&anchor, "").expect("anchor");
    let tsgo = std::fs::canonicalize(tsgo).expect("canonical tsgo");
    (install, anchor, tsgo)
}

/// Octocode's own tsgo serves TypeScript whatever the trust state, and
/// a tsgo the scanned checkout ships is never run untrusted.
#[cfg(unix)]
#[test]
fn the_bundled_tsgo_serves_typescript_in_an_untrusted_workspace() {
    use crate::lsp::config::catalog::default_server_for_file_from;
    use std::os::unix::fs::PermissionsExt;
    let (_install, anchor, tsgo) = install_with_bundled_tsgo();
    let workspace = TmpDir::new();
    let shipped = workspace.join("node_modules").join(".bin");
    std::fs::create_dir_all(&shipped).expect("bin dir");
    let shipped_tsgo = shipped.join("tsgo");
    std::fs::write(&shipped_tsgo, "#!/bin/sh\n").expect("shipped tsgo");
    std::fs::set_permissions(&shipped_tsgo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let root = workspace.to_str().expect("utf-8 root");
    let untrusted = LspDiscoveryOptions::default();

    for file in ["a.ts", "a.tsx", "a.mts", "a.cts"] {
        let config =
            default_server_for_file_from(file, root, &untrusted, Some(&anchor)).expect(file);
        assert_eq!(PathBuf::from(&config.command), tsgo, "{file}");
        assert_eq!(config.source, ServerSource::Bundled, "{file}");
        assert_eq!(
            config.args.as_deref(),
            Some(&["--lsp".to_owned(), "-stdio".to_owned()][..])
        );
    }
    // The Go GC works toward GOMEMLIMIT, set under the memory cap.
    let config = default_server_for_file_from("a.ts", root, &untrusted, Some(&anchor)).expect("ts");
    let cap = crate::lsp::spawn_limits::effective_memory_cap_mb(config.max_memory_mb)
        .expect("the default cap is on");
    assert_eq!(
        config.env.as_ref().and_then(|env| env.get("GOMEMLIMIT")),
        Some(&format!("{}MiB", cap - 512)),
        "{config:?}"
    );
    // JavaScript stays on tsserver.
    for file in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
        let js = default_server_for_file_from(file, root, &untrusted, Some(&anchor)).expect(file);
        assert_ne!(PathBuf::from(&js.command), tsgo, "{file}");
    }
}

/// A trusted workspace's own tsgo (its TypeScript version) still wins
/// over octocode's bundled one.
#[cfg(unix)]
#[test]
fn a_trusted_workspace_tsgo_wins_over_the_bundled_one() {
    use crate::lsp::config::catalog::default_server_for_file_from;
    use std::os::unix::fs::PermissionsExt;
    let (_install, anchor, bundled) = install_with_bundled_tsgo();
    let workspace = TmpDir::new();
    let shipped = workspace.join("node_modules").join(".bin");
    std::fs::create_dir_all(&shipped).expect("bin dir");
    let shipped_tsgo = shipped.join("tsgo");
    std::fs::write(&shipped_tsgo, "#!/bin/sh\n").expect("shipped tsgo");
    std::fs::set_permissions(&shipped_tsgo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let root = workspace.to_str().expect("utf-8 root");
    let trusted = LspDiscoveryOptions {
        trust_project_config: true,
        ..LspDiscoveryOptions::default()
    };
    let config = default_server_for_file_from("a.ts", root, &trusted, Some(&anchor)).expect("ts");
    assert_eq!(PathBuf::from(&config.command), shipped_tsgo);
    assert_ne!(PathBuf::from(&config.command), bundled);
}

/// Without a bundled tsgo the route stays tsserver, and the fallback is
/// offered only where a native route exists: a user server entry is the
/// user's explicit choice and has no fallback.
#[cfg(unix)]
#[test]
fn only_tsgo_serves_requests_concurrently() {
    let config = |command: &str| LspLanguageServerConfig {
        command: command.to_owned(),
        args: None,
        workspace_root: "/workspace".to_owned(),
        language_id: Some("typescript".to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    assert!(serves_requests_concurrently(&config(
        "/i/@typescript/native-preview-darwin-arm64/lib/tsgo"
    )));
    assert!(serves_requests_concurrently(&config("tsgo")));
    assert!(!serves_requests_concurrently(&config(
        "typescript-language-server"
    )));
    assert!(!serves_requests_concurrently(&config("gopls")));
}

/// tsgo answers `workspace/symbol` with at most 256 symbols
/// (typescript-go `internal/ls/symbols.go`); other servers have no
/// known cap.
#[test]
fn only_tsgo_caps_workspace_symbols() {
    let config = |command: &str| LspLanguageServerConfig {
        command: command.to_owned(),
        args: None,
        workspace_root: "/workspace".to_owned(),
        language_id: Some("typescript".to_owned()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    assert_eq!(
        crate::lsp::config::workspace_symbol_limit(&config(
            "/i/@typescript/native-preview-darwin-arm64/lib/tsgo"
        )),
        Some(256)
    );
    assert_eq!(
        crate::lsp::config::workspace_symbol_limit(&config("tsgo.cmd")),
        Some(256)
    );
    assert_eq!(
        crate::lsp::config::workspace_symbol_limit(&config("typescript-language-server")),
        None
    );
    assert_eq!(
        crate::lsp::config::workspace_symbol_limit(&config("gopls")),
        None
    );
}

#[cfg(unix)]
#[test]
fn tsserver_is_the_fallback_when_tsgo_is_absent_or_fails() {
    use crate::lsp::config::{
        catalog::default_server_for_file_from, catalog::typescript_server_fallback_from,
    };
    let (_install, anchor, tsgo) = install_with_bundled_tsgo();
    let bare = TmpDir::new();
    std::fs::create_dir_all(&bare).expect("bare install");
    let bare_anchor = bare.join("octocode");
    let workspace = TmpDir::new();
    std::fs::create_dir_all(&workspace).expect("workspace");
    let root = workspace.to_str().expect("utf-8 root");
    let options = LspDiscoveryOptions::default();

    let absent = default_server_for_file_from("a.ts", root, &options, Some(&bare_anchor))
        .expect("ts without tsgo");
    assert!(
        format!("{} {:?}", absent.command, absent.args).contains("typescript-language-server"),
        "{absent:?}"
    );
    assert!(
        typescript_server_fallback_from("a.ts", root, &options, Some(&bare_anchor)).is_none(),
        "nothing to fall back from"
    );

    let fallback = typescript_server_fallback_from("a.ts", root, &options, Some(&anchor))
        .expect("tsgo route has a tsserver fallback");
    assert_ne!(PathBuf::from(&fallback.command), tsgo);
    assert!(
        format!("{} {:?}", fallback.command, fallback.args).contains("typescript-language-server"),
        "{fallback:?}"
    );
    assert!(
        typescript_server_fallback_from("a.js", root, &options, Some(&anchor)).is_none(),
        "JavaScript never used tsgo"
    );

    let home = TmpDir::new();
    std::fs::create_dir_all(&home).expect("home");
    std::fs::write(
        home.join("lsp-servers.json"),
        r#"{"languageServers":{".ts":{"command":"my-ts","args":[],"languageId":"typescript"}}}"#,
    )
    .expect("user config");
    let user = LspDiscoveryOptions {
        octocode_home: Some(home.to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    assert!(
        typescript_server_fallback_from("a.ts", root, &user, Some(&anchor)).is_none(),
        "an explicit user entry has no silent fallback"
    );
}
