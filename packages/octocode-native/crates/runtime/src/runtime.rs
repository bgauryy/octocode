//! Request lifetime and bounded scheduling shared by both native interfaces.

mod auth;
pub(crate) mod dispatch;
pub(crate) mod domain_dispatch;
mod engine;
pub mod error;
mod github;
mod github_cache;
mod maintenance;
mod row_indices;
mod tool_output;

pub use crate::response::result::FailureKind;
pub use auth::{AuthStatus, LoginPlan, LoginTarget, Verification};
pub use engine::{Channel, HostOptions, RuntimeError, ToolRuntime};
