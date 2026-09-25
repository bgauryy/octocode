pub mod catalog;
pub mod cli;
pub mod database;
pub mod dispatch;
pub mod entities;
#[path = "../../octocode-config/rust/home.rs"]
mod home;
pub mod host_hooks;
mod mcp;
pub mod paths;
pub mod proxy;
pub mod store;
mod transport;
mod wire;
