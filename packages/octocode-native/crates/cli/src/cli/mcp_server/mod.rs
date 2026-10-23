//! `octocode mcp`: the MCP server over stdio. One native process serves
//! both protocol eras (a 2025-era client opens with `initialize`, a
//! 2026-07-28 client with `server/discover`) from the contract embedded in
//! this binary, so there is no second artifact to drift from the runtime.
mod handler;
mod listing;

use handler::Handler;
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use rmcp::ServiceExt;
use serde_json::{Value, json};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

/// How long a failed start waits for the client's first request.
const STARTUP_FAILURE_REPLY: Duration = Duration::from_secs(10);
/// How long shutdown waits for in-flight requests to unwind.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// A failed start answers the client's first request (its `initialize`) with
/// the reason, then exits, so the client shows why instead of a closed
/// connection. stderr keeps the same line for logs.
async fn fail_startup(message: &str) -> ! {
    eprintln!("{message}");
    let reply = async {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let id = serde_json::from_str::<Value>(&line)
                .ok()
                .and_then(|request| request.get("id").cloned())
                .filter(|id| id.is_string() || id.is_number());
            if let Some(id) = id {
                let error = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": message}});
                let mut stdout = std::io::stdout().lock();
                let _ = writeln!(stdout, "{error}").and_then(|()| stdout.flush());
                return;
            }
        }
    };
    let _ = tokio::time::timeout(STARTUP_FAILURE_REPLY, reply).await;
    // The stdin reader thread cannot be cancelled: leave without joining it.
    std::process::exit(1)
}

/// Resolves on the first SIGINT or SIGTERM.
async fn termination() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            return std::future::pending().await;
        };
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Serve until the client closes stdin or the process is told to stop, then
/// cancel what is in flight and close the runtime, waiting at most the
/// grace period for it to unwind.
pub(super) async fn run() -> ! {
    let host = HostOptions {
        surface: RuntimeSurface::Mcp,
        ..HostOptions::default()
    };
    let runtime = match ToolRuntime::from_host(host) {
        Ok(runtime) => Arc::new(runtime),
        Err(error) => {
            fail_startup(&format!("Server initialization failed: {}", error.message)).await
        }
    };
    let handler = octocode_native::contracts::cli_presentation()
        .map_err(str::to_owned)
        .and_then(|presentation| listing::build(presentation, &runtime.catalog()))
        .and_then(|listing| Handler::new(Arc::clone(&runtime), listing));
    let handler = match handler {
        Ok(handler) => handler,
        Err(message) => fail_startup(&format!("Server initialization failed: {message}")).await,
    };
    let service = match handler.serve(rmcp::transport::stdio_pipes()).await {
        Ok(service) => service,
        Err(error) => {
            eprintln!("[octocode-mcp] transport: {error}");
            runtime.close().await;
            std::process::exit(1)
        }
    };
    let stop = service.cancellation_token();
    tokio::spawn(async move {
        termination().await;
        stop.cancel();
    });
    let _ = service.waiting().await;
    if tokio::time::timeout(SHUTDOWN_GRACE, runtime.close())
        .await
        .is_err()
    {
        eprintln!("[octocode-mcp] shutdown: in-flight requests did not unwind in time");
    }
    // Where stdin is not a reactor pipe (every platform but Linux, and any
    // stream that is not a FIFO), its blocking reader thread cannot be
    // cancelled: dropping the runtime would wait for it, so the process ends
    // here.
    std::process::exit(0)
}
