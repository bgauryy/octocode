//! The request kernel every layer above shares: one request's execution
//! context, its errors and cancellation, the runtime's code limits, and
//! which tools this runtime serves. It names nothing above it, so response
//! shaping, providers and tools take it without reaching the engine.

pub mod availability;
pub(crate) mod cancel;
mod lifecycle;
mod limits;

pub(crate) use lifecycle::{Cost, ExecutionContext, MAX_TIMEOUT};
pub use lifecycle::{
    DEFAULT_TIMEOUT, ExecutionError, RequestAdmission, RequestLimits, RequestRuntime,
};
pub use limits::{CloneLimits, RuntimeLimits, SelectionLimits};
