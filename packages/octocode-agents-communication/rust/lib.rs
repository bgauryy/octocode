pub mod activity;
pub mod catalog;
pub mod cli;
mod completion;
pub mod database;
pub mod dispatch;
mod documents;
pub mod entities;
mod health;
// @octocodeai/config owns the home policy but ships no crate; rustc dep-info still
// tracks this file for rebuilds, and `publish = false` means packaging never needs it.
#[path = "../../octocode-config/rust/home.rs"]
mod home;
pub mod host_hooks;
mod lease_guard;
mod leases;
mod mcp;
pub mod paths;
pub mod proxy;
pub mod retention;
pub mod store;
mod transport;
mod wire;
