use super::*;
use crate::lsp::config::invocation::rustup::{RustupView, with_view};

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
            None,
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
    let identity = crate::lsp::managed::AssetIdentity::locked(
        "rust-analyzer",
        &crate::lsp::managed::platform_id(),
    )
    .expect("locked asset");
    std::fs::write(
        crate::lsp::managed::marker_path(&bin),
        crate::lsp::managed::cache_marker(&identity, &crate::digest::sha256(b"managed"), 7)
            .to_string(),
    )
    .expect("marker");
    let mut options = LspDiscoveryOptions {
        octocode_home: Some(home.to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    // No rustup on PATH: the managed install is the first source.
    let no_rustup = RustupView::default;
    let managed = with_view(no_rustup(), || {
        default_server_for_file("src/lib.rs", "/workspace", &options)
    })
    .expect("rust route");
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
    let unmanaged = with_view(no_rustup(), || {
        default_server_for_file("src/lib.rs", "/workspace", &options)
    })
    .expect("rust route");
    assert_ne!(PathBuf::from(&unmanaged.command), bin);
    assert_eq!(unmanaged.source, ServerSource::Path);
}

/// A rustup proxy on PATH whose toolchain for the workspace ships
/// rust-analyzer wins over a managed install; a toolchain without the
/// component leaves the managed install in place.
#[cfg(unix)]
#[test]
fn a_toolchain_matched_rustup_component_wins_over_the_managed_install() {
    use std::os::unix::fs::PermissionsExt;
    let base = TmpDir::new();
    let executable = |path: &std::path::Path| {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "#!/bin/sh\n").expect("write");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    };
    // rustup home: `pinned` ships the component, `bare` does not.
    let rustup_home = base.join("rustup");
    executable(&rustup_home.join("toolchains/pinned/bin/rust-analyzer"));
    std::fs::create_dir_all(rustup_home.join("toolchains/bare/bin")).expect("bare");
    std::fs::write(
        rustup_home.join("settings.toml"),
        "default_toolchain = \"bare\"\n",
    )
    .expect("settings");
    // PATH: rust-analyzer is a proxy link to rustup.
    let cargo_bin = base.join("cargo/bin");
    executable(&cargo_bin.join("rustup"));
    std::os::unix::fs::symlink("rustup", cargo_bin.join("rust-analyzer")).expect("proxy");
    // A managed install exists.
    let home = base.join("home");
    let platform = crate::lsp::managed::platform_id();
    let bin =
        crate::lsp::managed::cached_server_bin_path(&home.join("lsp"), "rust-analyzer", &platform)
            .expect("rust-analyzer ships for this platform");
    std::fs::create_dir_all(bin.parent().expect("bin dir")).expect("install dir");
    std::fs::write(&bin, b"managed").expect("binary");
    let identity = crate::lsp::managed::AssetIdentity::locked("rust-analyzer", &platform)
        .expect("locked asset");
    std::fs::write(
        crate::lsp::managed::marker_path(&bin),
        crate::lsp::managed::cache_marker(&identity, &crate::digest::sha256(b"managed"), 7)
            .to_string(),
    )
    .expect("marker");
    let options = LspDiscoveryOptions {
        octocode_home: Some(home),
        ..LspDiscoveryOptions::default()
    };
    let view = |toolchain: Option<&str>| RustupView {
        path_var: Some(cargo_bin.clone().into_os_string()),
        rustup_home: Some(rustup_home.clone()),
        toolchain: toolchain.map(str::to_owned),
    };
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let workspace = workspace.to_string_lossy().into_owned();
    let resolve = |toolchain: Option<&str>| {
        with_view(view(toolchain), || {
            default_server_for_file("src/lib.rs", &workspace, &options)
        })
        .expect("rust route")
    };

    // The default toolchain lacks the component: the managed install stays.
    assert_eq!(resolve(None).source, ServerSource::Managed);
    // The workspace's toolchain file selects one that ships it.
    std::fs::write(
        base.join("workspace/rust-toolchain.toml"),
        "[toolchain]\nchannel = \"pinned\"\n",
    )
    .expect("toolchain file");
    let rustup = resolve(None);
    assert_eq!(rustup.source, ServerSource::Path);
    assert_ne!(PathBuf::from(&rustup.command), bin);
    // RUSTUP_TOOLCHAIN overrides the file, as it does for rustup.
    assert_eq!(resolve(Some("bare")).source, ServerSource::Managed);
}
