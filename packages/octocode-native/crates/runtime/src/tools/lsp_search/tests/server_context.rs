//! Server context overlays: a Rust context over the engine's headless
//! defaults, and a default-equivalent context sharing the default server.

use super::*;

#[test]
fn rust_context_overlays_the_engine_headless_defaults() {
    let mut config = octocode_engine::lsp::types::LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: None,
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: Some(
            octocode_engine::lsp::config::rust_analyzer_headless_options(),
        ),
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let q = query(serde_json::json!({
        "operation": "definition", "path": "/repo/src/lib.rs",
        "symbolName": "foo", "line": 1,
        "rustContext": {"features": ["x"]}
    }));
    apply_rust_context(&mut config, &q).expect("rust context");
    let options = config.initialization_options.expect("options");
    assert_eq!(options["cargo"]["features"], serde_json::json!(["x"]));
    assert_eq!(options["cargo"]["buildScripts"]["enable"], false);
    assert_eq!(options["cargo"]["targetDir"], true);
    assert_eq!(options["procMacro"]["enable"], false);
    assert_eq!(options["checkOnSave"], false);
    assert_eq!(
        options["cachePriming"]["enable"], false,
        "engine defaults the overlay does not set are kept"
    );
    assert!(
        options.get("cfg").is_none(),
        "cfg(test) code stays analyzed: the overlay never sets cfg.setTest"
    );
    assert!(options["cargo"].get("cfgs").is_none());
    assert!(options["cargo"].get("noDefaultFeatures").is_none());
}

fn rust_server_config(
    context: serde_json::Value,
) -> octocode_engine::lsp::types::LspLanguageServerConfig {
    let mut config = octocode_engine::lsp::types::LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: None,
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: Some(
            octocode_engine::lsp::config::rust_analyzer_headless_options(),
        ),
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let mut row = serde_json::json!({
        "operation": "references", "path": "/repo/src/lib.rs", "symbolName": "foo", "line": 1
    });
    if !context.is_null() {
        row["rustContext"] = context;
    }
    apply_rust_context(&mut config, &query(row)).expect("rust context");
    config
}

/// A context that names only default values is the default analysis: the
/// same configuration, so the same pooled server (one rust-analyzer, not
/// one per spelling of the defaults). A context that changes one field
/// changes only that field.
#[test]
fn a_default_equivalent_rust_context_shares_the_default_server() {
    let key = |context| {
        octocode_engine::lsp::pool::canonical_lsp_key(&rust_server_config(context)).expect("key")
    };
    let default = key(serde_json::Value::Null);
    for explicit in [
        serde_json::json!({}),
        serde_json::json!({"buildScripts": false}),
        serde_json::json!({"buildScripts": false, "procMacros": false}),
        serde_json::json!({"features": [], "noDefaultFeatures": false}),
        serde_json::json!({"cfgs": ["debug_assertions"]}),
    ] {
        assert_eq!(key(explicit.clone()), default, "{explicit}");
    }
    let scripts = rust_server_config(serde_json::json!({"buildScripts": true}))
        .initialization_options
        .expect("options");
    assert_ne!(key(serde_json::json!({"buildScripts": true})), default);
    let mut expected = octocode_engine::lsp::config::rust_analyzer_headless_options();
    expected["cargo"]["buildScripts"]["enable"] = serde_json::json!(true);
    assert_eq!(scripts, expected, "only the named field changes");
    let cfgs = rust_server_config(serde_json::json!({"cfgs": ["tokio_unstable"]}))
        .initialization_options
        .expect("options");
    assert_eq!(
        cfgs["cargo"]["cfgs"],
        serde_json::json!(["debug_assertions", "miri", "tokio_unstable"]),
        "named cfgs add to rust-analyzer's defaults"
    );
}
