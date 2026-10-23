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
mod invocation;
#[cfg(unix)]
mod probe;
mod resolution;
mod server_defaults;
mod tsgo;
mod workspace_root;
