//! JSON-RPC transport to a language server.
//!
//! * `codec` — bounded `Content-Length` framing (read) and whole-frame encoding.
//! * `connection` — [`JsonRpcConnection`]: request/notify, pending map, cancel
//!   on drop, the single writer task, and the reader task.
//! * `patience` — how long a request outwaits a slow but progressing server.
//! * `progress` — `$/progress` tracking and [`Readiness`].
//! * `partial` — bounded partial-result collection and merge.
//! * `push_diagnostics` — bounded `publishDiagnostics` cache.
//! * `server_requests` — replies owed to server→client requests.
//!
//! Every size limit is owned by the store or codec it bounds.

mod codec;
mod connection;
mod partial;
mod patience;
mod progress;
mod push_diagnostics;
mod server_requests;

pub(crate) use connection::JsonRpcConnection;
pub use patience::with_caller_deadline;
pub(crate) use patience::{CEILING_FACTOR, CpuClock, WaitBudget};
pub(crate) use progress::{ProgressTracker, Readiness, STARTUP_SETTLE_MS};
pub(crate) use server_requests::ClientRequestContext;
