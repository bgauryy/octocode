//! A Rust server's configured analysis: the engine's headless defaults, a
//! configuration that runs build scripts, and one that cannot run.

use super::*;
use octocode_test_support::json_fixture;

/// A rust-analyzer configuration: the engine's headless defaults with
/// `options` merged over them, as a user's server configuration sets them.
pub(super) fn rust_server_config(
    options: serde_json::Value,
) -> octocode_engine::lsp::types::LspLanguageServerConfig {
    fn merge(into: &mut serde_json::Value, from: serde_json::Value) {
        match (into, from) {
            (serde_json::Value::Object(into), serde_json::Value::Object(from)) => {
                for (key, value) in from {
                    merge(into.entry(key).or_insert(serde_json::Value::Null), value);
                }
            }
            (into, from) => *into = from,
        }
    }
    let mut initialization = octocode_engine::lsp::config::rust_analyzer_headless_options();
    merge(&mut initialization, options);
    octocode_engine::lsp::types::LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: None,
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        document_language_ids: Default::default(),
        initialization_options: Some(initialization),
        env: Some(std::collections::HashMap::from([(
            "PATH".to_owned(),
            std::env::var("PATH").unwrap_or_default(),
        )])),
        max_memory_mb: None,
        source: Default::default(),
    }
}

/// The headless defaults run no build script and pass; a configuration that
/// runs build scripts, proc macros and every feature is read as such and
/// passes; proc macros without build scripts cannot run (rust-analyzer
/// builds them through Cargo) and are refused before a server starts. A
/// server of another language is not asked.
#[test]
fn a_rust_server_runs_proc_macros_only_with_build_scripts() {
    let headless = rust_server_config(serde_json::json!({}));
    assert!(validate_rust_options(&headless).is_ok());
    let analysis = RustAnalysis::of(&headless);
    assert!(!analysis.build_scripts && !analysis.all_features);

    let configured = rust_server_config(json_fixture(
        r#"{
            "cargo": {"features": "all", "buildScripts": {"enable": true}},
            "procMacro": {"enable": true}
        }"#,
    ));
    assert!(validate_rust_options(&configured).is_ok());
    let analysis = RustAnalysis::of(&configured);
    assert!(analysis.build_scripts && analysis.all_features);

    let invalid = rust_server_config(json_fixture(r#"{"procMacro": {"enable": true}}"#));
    assert!(
        validate_rust_options(&invalid)
            .unwrap_err()
            .contains("procMacro.enable requires cargo.buildScripts.enable")
    );
    let mut other = invalid;
    other.language_id = Some("typescript".into());
    assert!(validate_rust_options(&other).is_ok());
}
