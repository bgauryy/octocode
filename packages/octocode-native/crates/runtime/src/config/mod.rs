//! Deterministic native Octocode configuration.
mod acquire;
mod dotenv;
mod edit;
#[cfg(test)]
mod effect_tests;
mod json_edit;
#[cfg(test)]
mod layering_tests;
mod loader;
mod manage;
pub(crate) mod resolver;
mod types;
mod validation;
pub use acquire::acquire_config_input;
pub(crate) use acquire::octocode_home;
pub use dotenv::parse_env;
pub use edit::{
    backup_path, config_revision, edit_scoped_env, read_private_config, replace_private_config,
    validate_env_edit,
};
pub use json_edit::{edit_config_json, parse_config_json};
pub(crate) use loader::load_config;
pub use manage::{edit_setting, inspect_management};
#[cfg(test)]
pub(crate) use resolver::resolve_sections;
pub(crate) use resolver::{get_config_value, inspector_data, is_stats_enabled, nearest_tool_name};
pub use resolver::{is_persistent_storage_enabled, resolve_config};
pub use types::*;

#[cfg(test)]
mod tests;
