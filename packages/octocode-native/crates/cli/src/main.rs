// Test-compiled code may assert with unwrap/expect/panic. A crate attribute,
// not clippy.toml's `allow-*-in-tests`: clippy does not treat modules mounted
// under `cfg(all(test, unix))` as test code.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
mod cli;

// The CLI is a short-lived, IO-bound process: CPU-parallel work lives in the
// engine's `rayon` pools, not on tokio
// worker threads. A multi-thread runtime would spawn one worker per core per
// invocation only to leave them idle, so the CLI drives a single-threaded
// reactor; `octocode mcp` serves on it too.
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    match cli::parse_args() {
        Ok(args) => std::process::ExitCode::from(cli::run(args).await),
        Err(code) => std::process::ExitCode::from(code),
    }
}
