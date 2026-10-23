use super::*;

#[cfg(unix)]
#[test]
fn python_route_prefers_basedpyright_then_pyright_then_pylsp() {
    use std::os::unix::fs::PermissionsExt;
    let root = TmpDir::new();
    let path_dir = root.join("path-bin");
    let workspace = root.join("workspace");
    let workspace_bin = workspace.join("node_modules").join(".bin");
    std::fs::create_dir_all(&path_dir).expect("path dir");
    std::fs::create_dir_all(&workspace_bin).expect("workspace bin");
    let fake = |dir: &std::path::Path, name: &str| {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").expect("fake server");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    };
    let workspace_str = workspace.to_string_lossy().into_owned();
    let resolve = || {
        crate::lsp::config::invocation::resolve_pyright_family(
            &workspace_str,
            Some(path_dir.as_os_str()),
        )
    };

    // Nothing installed: the caller falls back to pylsp.
    assert_eq!(resolve(), None);

    // A checkout-shipped server is never picked automatically.
    fake(&workspace_bin, "pyright-langserver");
    fake(&workspace_bin, "basedpyright-langserver");
    assert_eq!(resolve(), None);

    // pyright on PATH is found.
    let path_pyright = fake(&path_dir, "pyright-langserver");
    let (command, args) = resolve().expect("PATH pyright");
    assert_eq!(PathBuf::from(command), path_pyright);
    assert_eq!(args, ["--stdio"]);

    // basedpyright beats pyright.
    let path_based = fake(&path_dir, "basedpyright-langserver");
    assert_eq!(
        resolve().map(|(c, _)| PathBuf::from(c)),
        Some(path_based.clone())
    );

    // A non-executable file is not a server.
    std::fs::set_permissions(&path_based, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    assert_eq!(
        resolve().map(|(c, _)| PathBuf::from(c)),
        Some(path_dir.join("pyright-langserver"))
    );
}

#[test]
fn managed_install_resolves_ahead_of_path_and_behind_an_override() {
    let home = TmpDir::new();
    let bin = crate::lsp::managed::cached_server_bin_path(
        &home.join("lsp"),
        "rust-analyzer",
        &crate::lsp::managed::platform_id(),
    )
    .expect("rust-analyzer ships for this platform");
    std::fs::create_dir_all(bin.parent().expect("bin dir")).expect("install dir");
    std::fs::write(&bin, b"managed").expect("binary");
    std::fs::write(
        crate::lsp::managed::marker_path(&bin),
        json!({
            "assetSha256": crate::lsp::managed::manifest()["rust-analyzer"].platforms
                [crate::lsp::managed::platform_id().as_str()]
            .sha256
            .expect("pinned asset"),
            "binarySha256": crate::digest::sha256(b"managed"),
            "size": 7,
        })
        .to_string(),
    )
    .expect("marker");
    let mut options = LspDiscoveryOptions {
        octocode_home: Some(home.to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    let managed =
        default_server_for_file("src/lib.rs", "/workspace", &options).expect("rust route");
    assert_eq!(PathBuf::from(&managed.command), bin);
    assert_eq!(managed.source, ServerSource::Managed);
    assert!(
        managed.initialization_options.is_some(),
        "headless defaults"
    );

    options.env = BTreeMap::from([(
        "OCTOCODE_LSP_CACHE_DIR".to_owned(),
        home.join("elsewhere").to_string_lossy().into_owned(),
    )]);
    let unmanaged =
        default_server_for_file("src/lib.rs", "/workspace", &options).expect("rust route");
    assert_ne!(PathBuf::from(&unmanaged.command), bin);
    assert_eq!(unmanaged.source, ServerSource::Path);
}
