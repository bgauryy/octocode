use super::{
    LspDiscoveryOptions, default_server_for_file, default_server_for_workspace_root,
    detect_language_id, invocation::is_node_executable, invocation::resolve_known_server_command,
    is_rust_analyzer_command, workspace_root_representative_source,
};
#[cfg(unix)]
use super::{
    command_resolves_to_executable, current_node_command, invocation::resolve_server_invocation,
    invocation::resolve_server_invocation_with_environment, is_command_available,
    serves_requests_concurrently,
};
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use crate::test_support::TmpDir;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn write_lsp_config(root: &std::path::Path, json: &str) {
    let config_dir = root.join(".octocode");
    std::fs::create_dir_all(&config_dir).expect("create .octocode dir");
    std::fs::write(config_dir.join("lsp-servers.json"), json).expect("write lsp config");
}

mod catalog;
mod discovery;
#[cfg(unix)]
mod fetched_code;
mod invocation;
#[cfg(unix)]
mod probe;
mod repository_code;
mod resolution;
#[cfg(unix)]
mod rustup_fixture;
#[cfg(unix)]
mod rustup_memo;
#[cfg(unix)]
mod rustup_toolchain_path;
mod server_defaults;
mod tsgo;
mod workspace_root;

/// The extension list `lsp-server status` walks names only real routes, and
/// no extension the catalog routes is missing from it.
#[test]
fn routed_extensions_match_the_catalog() {
    use super::catalog::{ROUTED_EXTENSIONS, spec_for_extension};
    for extension in ROUTED_EXTENSIONS {
        assert!(spec_for_extension(extension).is_some(), "{extension}");
    }
    let source = include_str!("catalog.rs");
    let body = &source[source.find("fn spec_for_extension").expect("catalog match")..];
    let routed = body
        .lines()
        .take_while(|line| !line.starts_with("}"))
        .filter(|line| line.contains("=> ServerSpec"))
        .flat_map(|line| line.split('"').skip(1).step_by(2));
    for extension in routed {
        assert!(ROUTED_EXTENSIONS.contains(&extension), "{extension}");
    }
}
