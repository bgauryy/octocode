//! Request lifetime and bounded scheduling shared by both native interfaces.

pub(crate) mod dispatch;
pub(crate) mod domain_dispatch;
pub(crate) mod engine;
pub mod error;
mod github;
mod github_cache;
mod maintenance;
mod row_indices;
mod tool_output;

pub use crate::response::result::FailureKind;
pub use engine::{Channel, HostOptions, RuntimeError, ToolRuntime};
