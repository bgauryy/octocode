use super::*;
use crate::lsp::config::scip_consent;

fn rust_options(config: &LspLanguageServerConfig) -> serde_json::Value {
    config.initialization_options.clone().expect("options")
}

fn user_home(options: serde_json::Value) -> (TmpDir, LspDiscoveryOptions) {
    let home = TmpDir::new();
    let entry = json!({"languageServers": {".rs": {
        "command": "rust-analyzer", "languageId": "rust",
        "initializationOptions": options,
    }}});
    home.write("lsp-servers.json", entry.to_string());
    let discovery = LspDiscoveryOptions {
        octocode_home: Some(home.to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    (home, discovery)
}

fn consents(user: serde_json::Value) -> bool {
    let (home, discovery) = user_home(user);
    let config = default_server_for_file("x.rs", &home.path(), &discovery).expect("rust route");
    scip_consent(&rust_options(&config))
}

#[test]
fn defaults_do_not_consent() {
    let config = default_server_for_file("x.rs", "/workspace", &LspDiscoveryOptions::default())
        .expect("rust route");
    assert!(!scip_consent(&rust_options(&config)));
}

#[test]
fn consent_needs_build_scripts_and_proc_macros_together() {
    assert!(!consents(
        json!({"cargo": {"buildScripts": {"enable": true}}})
    ));
    assert!(!consents(json!({"procMacro": {"enable": true}})));
    assert!(consents(json!({
        "cargo": {"buildScripts": {"enable": true}},
        "procMacro": {"enable": true},
    })));
    // Only a boolean true counts.
    assert!(!consents(json!({
        "cargo": {"buildScripts": {"enable": "true"}},
        "procMacro": {"enable": true},
    })));
}

#[test]
fn an_untrusted_project_file_cannot_consent() {
    let project = TmpDir::new();
    project.write(
        ".octocode/lsp-servers.json",
        json!({"languageServers": {".rs": {
            "command": "rust-analyzer", "languageId": "rust",
            "initializationOptions": {
                "cargo": {"buildScripts": {"enable": true}},
                "procMacro": {"enable": true},
            },
        }}})
        .to_string(),
    );
    let root = project.path();
    let untrusted = LspDiscoveryOptions::default();
    let config = default_server_for_file("x.rs", &root, &untrusted).expect("rust route");
    assert!(!scip_consent(&rust_options(&config)));
    let trusted = LspDiscoveryOptions {
        trust_project_config: true,
        ..LspDiscoveryOptions::default()
    };
    let config = default_server_for_file("x.rs", &root, &trusted).expect("rust route");
    assert!(scip_consent(&rust_options(&config)));
}

#[test]
fn fetched_code_roots_are_refused() {
    let (home, mut discovery) = user_home(json!({
        "cargo": {"buildScripts": {"enable": true}},
        "procMacro": {"enable": true},
    }));
    discovery.fetched_code_roots = vec![home.to_path_buf()];
    assert!(default_server_for_file("x.rs", &home.path(), &discovery).is_none());
}
