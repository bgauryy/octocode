//! Single integration-test binary for octocode-cli.
//!
//! Every `tests/*.rs` file is a module here instead of its own test target
//! (`autotests = false`). Separate targets each linked a full copy of the crate
//! and its dependencies per build variant: 8 binaries, ~2 GB per build, and
//! hundreds of GB of stale copies in target/. One binary links once and runs
//! all tests in one parallel harness. Run one file with
//! `cargo dev-test -- <module>::` (`.cargo/config.toml`): the `test:rust`
//! selection, so it reuses that build instead of compiling another copy.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration-test assertions"
)]

mod support;

mod auth_discovery;
mod cli;
mod cli_batch_response;
mod cli_catalog;
mod cli_exit_codes;
mod cli_output;
mod cli_skill;
mod cli_tool_contract;
mod config_edit;
mod config_view;
mod system_commands;
mod warm_server;

/// `autotests = false` means a `tests/*.rs` file missing from the list above
/// would silently never run.
#[test]
fn every_test_file_is_a_module() {
    support::assert_every_test_file_is_a_module(env!("CARGO_MANIFEST_DIR"));
}
