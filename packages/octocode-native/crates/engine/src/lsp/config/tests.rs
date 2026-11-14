use super::{
    LspDiscoveryOptions, default_server_for_directory, default_server_for_file, detect_language_id,
    directory_languages, directory_representative, invocation::is_node_executable,
    invocation::resolve_known_server_command, is_rust_analyzer_command,
    workspace_root_representative_source,
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
mod route_memo;
#[cfg(unix)]
mod rustup_fixture;
#[cfg(unix)]
mod rustup_memo;
#[cfg(unix)]
mod rustup_toolchain_path;
mod server_defaults;
mod tsgo;
mod workspace_root;

/// Each routed extension has one built-in server: no extension is listed
/// under two catalog entries.
#[test]
fn each_routed_extension_has_one_route() {
    let mut extensions = super::catalog::ROUTED_EXTENSIONS.clone();
    extensions.sort_unstable();
    extensions.dedup();
    assert_eq!(extensions.len(), super::catalog::ROUTED_EXTENSIONS.len());
}
