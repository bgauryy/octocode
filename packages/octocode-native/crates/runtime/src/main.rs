// Test code (cli submodule tests) may assert with unwrap/expect/panic.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

mod cli;

use clap::Parser;

// The CLI is a short-lived, IO-bound process: CPU-parallel work lives in the
// engine's `rayon` pools and the out-of-process regex worker, not on tokio
// worker threads. A multi-thread runtime would spawn one worker per core per
// invocation only to leave them idle, so the CLI drives a single-threaded
// reactor. (The MCP/N-API path is unaffected — napi owns its own tokio runtime.)
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(cli::run(cli::Args::parse()).await)
}
