mod catalog;
mod defaults;
mod invocation;
mod probe;
mod types;
mod workspace_root;

#[cfg(all(test, unix))]
use crate::lsp::commands::command_resolves_to_executable;
#[cfg(test)]
use crate::lsp::types::LspLanguageServerConfig;
pub(crate) use catalog::configuration_section;
pub use catalog::install_hint;
pub use catalog::{
    available_server_languages, default_server_for_file, detect_language_id,
    typescript_server_fallback,
};
pub(crate) use defaults::{apply_server_default_options, project_config_files};
pub use defaults::{
    rust_analyzer_headless_options, serves_requests_concurrently, workspace_symbol_limit,
};
pub(super) use invocation::current_node_command;
pub(crate) use invocation::is_rust_analyzer_command;
pub use probe::{is_command_available, server_command_resolves};
pub use types::LspDiscoveryOptions;
pub use workspace_root::{
    default_server_for_workspace_root, representative_source_for,
    typescript_server_fallback_for_workspace_root, workspace_root_languages,
    workspace_root_marker_names, workspace_root_representative_source,
};

#[cfg(test)]
mod tests;
