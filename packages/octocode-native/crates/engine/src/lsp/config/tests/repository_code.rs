//! What a repository can make a language server run, beyond its documented
//! opt-ins: closed by the headless defaults.
use super::*;
use serde_json::json;

fn rust_analyzer(options: Option<serde_json::Value>) -> LspLanguageServerConfig {
    let mut config = LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: None,
        workspace_root: "/w".into(),
        language_id: Some("rust".into()),
        document_language_ids: Default::default(),
        initialization_options: options,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    crate::lsp::config::apply_server_default_options(&mut config, None, None);
    config
}

/// `cargo metadata` still runs with build scripts off, and through a repository's
/// `.cargo/config.toml` `rustc-wrapper`: an empty variable overrides that setting.
#[test]
fn rust_analyzer_runs_no_repository_rustc_wrapper_by_default() {
    let config = rust_analyzer(None);
    let options = config.initialization_options.expect("options");
    for wrapper in ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"] {
        assert_eq!(
            options["cargo"]["extraEnv"][wrapper],
            json!(""),
            "{options}"
        );
    }
}

/// Build scripts already run repository code, so the user's wrapper (sccache) runs
/// with them; a value the user set stays.
#[test]
fn build_scripts_keep_the_repository_wrapper_and_user_values_stay() {
    let built = rust_analyzer(Some(json!({"cargo": {"buildScripts": {"enable": true}}})));
    let options = built.initialization_options.expect("options");
    assert!(
        options.pointer("/cargo/extraEnv/RUSTC_WRAPPER").is_none(),
        "{options}"
    );
    let own = rust_analyzer(Some(
        json!({"cargo": {"extraEnv": {"RUSTC_WRAPPER": "sccache", "FOO": "1"}}}),
    ));
    let options = own.initialization_options.expect("options");
    assert_eq!(
        options["cargo"]["extraEnv"]["RUSTC_WRAPPER"],
        json!("sccache")
    );
    assert_eq!(options["cargo"]["extraEnv"]["FOO"], json!("1"));
    assert_eq!(
        options["cargo"]["extraEnv"]["RUSTC_WORKSPACE_WRAPPER"],
        json!("")
    );
}

/// A TypeScript install tree with its server CLI, and a workspace that ships its
/// own `tsserver.js`.
#[cfg(unix)]
fn typescript_layout() -> (crate::test_support::TmpDir, LspLanguageServerConfig, String) {
    let dir = crate::test_support::TmpDir::new();
    let install = dir.0.join("install/node_modules");
    let cli = install.join("typescript-language-server/lib/cli.mjs");
    let pinned = install.join("typescript/lib/tsserver.js");
    let workspace = dir.0.join("ws");
    for file in [
        &cli,
        &pinned,
        &workspace.join("node_modules/typescript/lib/tsserver.js"),
    ] {
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
        std::fs::write(file, "").expect("file");
    }
    let config = LspLanguageServerConfig {
        command: "node".into(),
        args: Some(vec![cli.to_string_lossy().into_owned(), "--stdio".into()]),
        workspace_root: workspace.to_string_lossy().into_owned(),
        language_id: Some("javascript".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let pinned = std::fs::canonicalize(&pinned).expect("canonical");
    (dir, config, pinned.to_string_lossy().into_owned())
}

/// An untrusted workspace's tsserver never runs: the server's own is pinned.
#[cfg(unix)]
#[test]
fn an_untrusted_workspace_runs_the_servers_own_tsserver() {
    let (_dir, mut config, pinned) = typescript_layout();
    super::super::invocation::typescript_cli_pin_for_tests(&mut config, false);
    assert_eq!(
        config.initialization_options.expect("options")["tsserver"]["path"],
        json!(pinned)
    );
}

/// A trusted workspace keeps its own TypeScript version, and a configured path
/// stays.
#[cfg(unix)]
#[test]
fn a_trusted_workspace_or_a_configured_path_is_left_alone() {
    let (_dir, mut trusted, _) = typescript_layout();
    super::super::invocation::typescript_cli_pin_for_tests(&mut trusted, true);
    assert!(trusted.initialization_options.is_none());
    let (_dir, mut own, _) = typescript_layout();
    own.initialization_options = Some(json!({"tsserver": {"path": "/mine/tsserver.js"}}));
    super::super::invocation::typescript_cli_pin_for_tests(&mut own, false);
    assert_eq!(
        own.initialization_options.expect("options")["tsserver"]["path"],
        json!("/mine/tsserver.js")
    );
}
