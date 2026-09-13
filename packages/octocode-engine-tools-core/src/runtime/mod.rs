//! Request lifetime and bounded scheduling shared by both native interfaces.

mod cursor;
mod engine;
pub mod error;
mod lifecycle;
pub mod render;
pub mod response;

pub use engine::{FailureKind, HostOptions, RuntimeError, ToolOutcome, ToolRuntime};
pub use lifecycle::{ExecutionContext, ExecutionError, RequestRuntime, RuntimeLimits};
