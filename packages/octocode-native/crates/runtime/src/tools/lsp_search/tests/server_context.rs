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
    assert!(
        options.get("cachePriming").is_none(),
        "the overlay sets no option the engine leaves at rust-analyzer's default"
    );
    assert!(
        options.get("cfg").is_none(),
        "cfg(test) code stays analyzed: the overlay never sets cfg.setTest"
    );
    assert!(options["cargo"].get("cfgs").is_none());
    assert!(options["cargo"].get("noDefaultFeatures").is_none());
}

pub(super) fn rust_server_config(
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

#[test]
fn rust_context_admission_uses_the_effective_configuration() {
    let configured = rust_server_config(serde_json::json!({
        "features":"all", "buildScripts":true, "procMacros":true
    }));
    let key = octocode_engine::lsp::pool::canonical_lsp_key(&configured).expect("configured key");
    let base = serde_json::json!({
        "operation":"references", "path":"/repo/src/lib.rs", "symbolName":"foo", "line":1
    });
    for context in [
        serde_json::Value::Null,
        serde_json::json!({}),
        serde_json::json!({"procMacros":true}),
        serde_json::json!({"features":"all", "buildScripts":true, "procMacros":true}),
    ] {
        let mut input = base.clone();
        if !context.is_null() {
            input["rustContext"] = context;
        }
        let query = query(input);
        let before = lsp_search_query::to_row(&query);
        let mut config = configured.clone();
        apply_rust_context(&mut config, &query).expect("inherited enabled build scripts");
        assert_eq!(
            octocode_engine::lsp::pool::canonical_lsp_key(&config).expect("key"),
            key
        );
        assert_eq!(lsp_search_query::to_row(&query), before);
        assert!(RustAnalysis::of(&config).build_scripts && RustAnalysis::of(&config).all_features);
    }

    for context in [
        serde_json::json!({"buildScripts":false}),
        serde_json::json!({"buildScripts":false, "procMacros":true}),
    ] {
        let mut input = base.clone();
        input["rustContext"] = context;
        let mut config = configured.clone();
        assert!(
            apply_rust_context(&mut config, &query(input))
                .unwrap_err()
                .contains("requires buildScripts:true")
        );
        assert_eq!(
            config.initialization_options, configured.initialization_options,
            "rejection cannot mutate the resolved config"
        );
    }

    let mut invalid_inherited = configured.clone();
    invalid_inherited
        .initialization_options
        .as_mut()
        .expect("options")["cargo"]["buildScripts"]["enable"] = serde_json::json!(false);
    assert!(apply_rust_context(&mut invalid_inherited, &query(base.clone())).is_err());
    let mut disabled = configured.clone();
    let mut input = base;
    input["rustContext"] =
        serde_json::json!({"features":[], "buildScripts":false, "procMacros":false});
    apply_rust_context(&mut disabled, &query(input)).expect("explicit disable");
    let options = disabled.initialization_options.as_ref().expect("options");
    assert_eq!(options["cargo"]["features"], serde_json::json!([]));
    assert_eq!(options["cargo"]["buildScripts"]["enable"], false);
    assert_eq!(options["procMacro"]["enable"], false);
    assert!(
        !RustAnalysis::of(&disabled).build_scripts && !RustAnalysis::of(&disabled).all_features
    );
    assert_ne!(
        octocode_engine::lsp::pool::canonical_lsp_key(&disabled).expect("key"),
        key
    );
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
