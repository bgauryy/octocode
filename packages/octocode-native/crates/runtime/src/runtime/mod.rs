//! Request lifetime and bounded scheduling shared by both native interfaces.

pub(crate) mod cursor;
mod dispatch;
mod domain_dispatch;
mod engine;
pub mod error;
mod github;
mod github_cache;
mod clasify_batch;
mod clasify_context;
mod lifecycle;
mod maintenance;
pub mod render;
pub mod response;
mod session_stats;

pub use cursor::CursorError;
pub use engine::{FailureKind, HostOptions, RuntimeError, ToolOutcome, ToolRuntime};
pub use lifecycle::{
    ExecutionContext, ExecutionError, RequestAdmission, RequestRuntime, RuntimeLimits,
};
