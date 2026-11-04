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

/// A rustup proxy on PATH whose selected toolchain (as `rustup which`
/// answers, run in the workspace with auto-install off) ships rust-analyzer
/// wins over a managed install; a toolchain without the component leaves the
/// managed install in place.
#[cfg(unix)]
#[test]
fn a_toolchain_matched_rustup_component_wins_over_the_managed_install() {
    use super::rustup_fixture::{FakeRustup, toolchain_binary};
    let base = TmpDir::new();
    let rustup = FakeRustup::install(&base);
    let component = toolchain_binary(&base, "rustup-home/toolchains/pinned/bin/rust-analyzer");
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
    let view = RustupView {
        path_var: Some(rustup.bin().as_os_str().to_owned()),
    };
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let workspace_str = workspace.to_string_lossy().into_owned();
    let resolve = || {
        with_view(view.clone(), || {
            default_server_for_file("src/lib.rs", &workspace_str, &options)
        })
        .expect("rust route")
    };

    // rustup selects a toolchain without the component: the managed install
    // stays.
    assert_eq!(resolve().source, ServerSource::Managed);
    // rustup selects one that ships it: its binary runs through the proxy.
    rustup.ships(&component);
    let selected = resolve();
    assert_eq!(selected.source, ServerSource::Path);
    assert_ne!(PathBuf::from(&selected.command), bin);
    // The selection was asked of rustup in the workspace root, with
    // RUSTUP_AUTO_INSTALL=0 (the fake refuses otherwise, which would have
    // kept the managed install above).
    assert_eq!(
        rustup.ran_in(),
        workspace.canonicalize().expect("canonical")
    );
    // The toolchain disappearing again falls back to the managed install.
    rustup.lacks_component();
    assert_eq!(resolve().source, ServerSource::Managed);
}
