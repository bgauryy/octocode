//! A repository's toolchain file can name a toolchain by absolute `path`, and the
//! rustup proxy would then run that directory's `rust-analyzer`: the repository's
//! own program. Such a build is not taken as the server.
use super::rustup_fixture::{FakeRustup, toolchain_binary};
use super::{RustupView, view_env, with_view};
use crate::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, repository_toolchain_refusal,
    server_command_resolves,
};
use crate::test_support::TmpDir;

/// Whether a `.rs` file in `<root>/project` gets a server, and the refusal its
/// route gives, with `toolchain_file`
/// as its `rust-toolchain.toml` and rustup answering `selected` (relative to
/// `root`), under `RUSTUP_TOOLCHAIN` when `env_toolchain` is set.
fn route(toolchain_file: &str, selected: &str, env_toolchain: bool) -> (bool, Option<String>) {
    let root = TmpDir::new();
    let rustup = FakeRustup::install(&root);
    rustup.ships(&toolchain_binary(&root, selected));
    let project = root.join("project");
    root.write(
        "project/rust-toolchain.toml",
        toolchain_file.replace("{root}", &root.0.to_string_lossy()),
    );
    let view = RustupView {
        path_var: Some(rustup.path()),
        home: Some(root.join("rustup-home")),
        toolchain: env_toolchain.then(|| "stable".into()),
    };
    let view_for_env = view.clone();
    with_view(view, || {
        let config = default_server_for_file(
            "src/lib.rs",
            &project.to_string_lossy(),
            &LspDiscoveryOptions {
                env: view_env(&view_for_env),
                ..LspDiscoveryOptions::default()
            },
        );
        (
            config.as_ref().is_some_and(server_command_resolves),
            config.as_ref().and_then(repository_toolchain_refusal),
        )
    })
}

const BY_PATH: &str = "[toolchain]\npath = \"{root}/project/tc\"\n";
const IN_REPOSITORY: &str = "project/tc/bin/rust-analyzer";
const INSTALLED: &str = "rustup-home/toolchains/stable/bin/rust-analyzer";

/// Not as the component, and not through the proxy either: the route refuses,
/// naming the file.
#[test]
fn a_toolchain_the_repository_names_by_path_is_not_run() {
    let (runs, refusal) = route(BY_PATH, IN_REPOSITORY, false);
    assert!(!runs);
    assert!(
        refusal.is_some_and(|refusal| refusal.contains("rust-toolchain.toml")),
        "the refusal names the file"
    );
}

#[test]
fn an_installed_toolchain_still_serves() {
    assert_eq!(
        route("[toolchain]\nchannel = \"stable\"\n", INSTALLED, false),
        (true, None)
    );
    // RUSTUP_TOOLCHAIN selects ahead of the file: the user chose that build.
    assert_eq!(route(BY_PATH, INSTALLED, true), (true, None));
}
