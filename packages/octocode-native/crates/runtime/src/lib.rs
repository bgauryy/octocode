//! Native execution shared by the CLI and the MCP addon. Every tool runs
//! here; the interfaces have no execution fallback.

pub mod cache;
mod civil_date;
pub mod config;
pub(crate) mod content;
pub mod contracts;
mod digest;
pub mod execution;
pub mod id;
pub(crate) mod num;
pub(crate) mod policy;
mod private_file;
#[cfg(test)]
mod process_fixture;
#[cfg(test)]
mod process_status;
pub mod providers;
pub mod response;
pub mod runtime;
pub mod security;
pub mod tools;
