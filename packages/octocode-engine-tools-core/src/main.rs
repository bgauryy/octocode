use clap::Parser;
use octocode_engine_tools_core::cli::{Args, run};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(run(Args::parse()).await)
}
