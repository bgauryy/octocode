//! Request lifetime and bounded scheduling shared by both native interfaces.

pub mod channels;
mod clasify_batch;
mod clasify_compact;
mod clasify_context;
mod clasify_handoff;
mod clasify_locate;
mod clasify_output;
mod continuations;
pub(crate) mod cursor;
mod dispatch;
mod domain_dispatch;
mod engine;
pub mod error;
mod github;
mod github_cache;
mod github_output;
mod lifecycle;
mod maintenance;
pub mod numbered;
mod page_warnings;
pub mod render;
pub mod response;
mod response_stage;
mod session_stats;
pub(crate) mod symbol_outline;
mod verbose;

pub use cursor::CursorError;
pub use engine::{FailureKind, HostOptions, RuntimeError, ToolOutcome, ToolRuntime};
pub use lifecycle::{
    ExecutionContext, ExecutionError, RequestAdmission, RequestRuntime, RuntimeLimits,
};
