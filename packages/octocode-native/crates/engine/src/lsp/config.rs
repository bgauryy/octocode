mod catalog;
mod defaults;
mod directory;
mod invocation;
mod kept;
mod probe;
mod route;
mod types;
mod user_config;
mod workspace_root;

#[cfg(all(test, unix))]
use crate::lsp::commands::command_resolves_to_executable;
#[cfg(test)]
use crate::lsp::types::LspLanguageServerConfig;
pub(crate) use catalog::configuration_section;
pub use catalog::install_hint;
pub use catalog::{
    ROUTED_EXTENSIONS, available_server_languages, default_server_for_file, detect_language_id,
    typescript_server_fallback,
};
pub use defaults::{allow_build_wrappers, rust_analyzer_headless_options};
pub(crate) use defaults::{
    apply_server_default_options, project_config_files, serves_requests_concurrently,
    workspace_symbol_cap_advice, workspace_symbol_limit,
};
pub use directory::{
    default_server_for_directory, directory_languages, directory_representative,
    typescript_server_fallback_for_directory,
};
pub(super) use invocation::current_node_command;
pub(crate) use invocation::{is_generic_interpreter_command, is_rust_analyzer_command};
pub use probe::{is_command_available, repository_toolchain_refusal, server_command_resolves};
pub use types::{FETCHED_CODE_REFUSAL, LspDiscoveryOptions};
pub use user_config::{
    UserConfigReport, route_problems, user_config_problems_for, user_config_reports,
};
pub use workspace_root::{
    representative_source_for, workspace_root_languages, workspace_root_marker_names,
    workspace_root_representative_source,
};

#[cfg(test)]
mod tests;
