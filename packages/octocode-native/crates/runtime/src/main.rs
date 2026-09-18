mod cli;

use clap::Parser;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(cli::run(cli::Args::parse()).await)
}
