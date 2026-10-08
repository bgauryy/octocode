//! JSON-RPC transport to a language server.
//!
//! * `codec` — bounded `Content-Length` framing (read) and whole-frame encoding.
//! * `connection` — [`JsonRpcConnection`]: request/notify, pending map, cancel
//!   on drop, the single writer task, and the reader task.
//! * `progress` — `$/progress` tracking and [`Readiness`].
//! * `partial` — bounded partial-result collection and merge.
//! * `push_diagnostics` — bounded `publishDiagnostics` cache.
//! * `server_requests` — replies owed to server→client requests.
//!
//! Every size limit is owned by the store or codec it bounds.

mod codec;
mod connection;
mod partial;
mod progress;
mod push_diagnostics;
mod server_requests;

pub(crate) use connection::JsonRpcConnection;
pub(crate) use progress::{ProgressTracker, Readiness};
pub(crate) use server_requests::{ClientRequestContext, configuration_section_for_command};
