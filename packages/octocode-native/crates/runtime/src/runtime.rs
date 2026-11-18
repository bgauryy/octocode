//! Request lifetime and bounded scheduling shared by both native interfaces.

mod auth;
#[cfg(test)]
mod config_effect_tests;
pub(crate) mod dispatch;
pub(crate) mod domain_dispatch;
mod engine;
pub mod error;
mod github;
mod maintenance;
mod row_indices;
pub(crate) mod tool_output;

pub use auth::{AuthStatus, LoginPlan, LoginTarget, Verification};
pub use engine::{Channel, HostOptions, RuntimeError, ToolRuntime};
pub use octocode_runtime_base::response::result::ErrorClass;
