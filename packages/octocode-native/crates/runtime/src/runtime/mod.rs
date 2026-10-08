//! Request lifetime and bounded scheduling shared by both native interfaces.

pub(crate) mod dispatch;
pub(crate) mod domain_dispatch;
pub(crate) mod engine;
pub mod error;
mod exit;
pub(crate) mod git_head;
mod github;
mod github_cache;
mod lifecycle;
mod maintenance;
mod row_indices;
mod source_identity;
mod tool_output;

pub use crate::tools::result::FailureKind;
pub use engine::{HostOptions, RuntimeError, ToolOutcome, ToolRuntime};
pub use exit::ExitClass;
pub use lifecycle::{
    ExecutionContext, ExecutionError, RequestAdmission, RequestRuntime, RuntimeLimits,
};
