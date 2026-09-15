//! Request lifetime and bounded scheduling shared by both native interfaces.

mod cursor;
mod dispatch;
mod engine;
pub mod error;
mod github;
mod github_cache;
mod lifecycle;
mod maintenance;
pub mod render;
pub mod response;

pub use engine::{FailureKind, HostOptions, RuntimeError, ToolOutcome, ToolRuntime};
pub use lifecycle::{
    ExecutionContext, ExecutionError, RequestAdmission, RequestRuntime, RuntimeLimits,
};
