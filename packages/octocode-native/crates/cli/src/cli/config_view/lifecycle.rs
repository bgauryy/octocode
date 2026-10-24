use super::{HEADER_TIMEOUT, State, Tokens, backend, handle, token};
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::convert::Infallible;
use std::io;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CONNECTIONS: usize = 32;

pub(in crate::cli) async fn run(no_open: bool, idle_timeout: u64, json_out: bool) -> u8 {
    match serve(no_open, Duration::from_secs(idle_timeout), json_out).await {
        Ok(code) => code,
        Err(()) => {
            crate::cli::emit_error(
                ErrorCode::ExecutionFailed,
                "Cannot open the config view. Check the Octocode installation and browser opener.",
                json_out,
            );
            5
        }
    }
}

async fn serve(no_open: bool, idle_timeout: Duration, json_out: bool) -> Result<u8, ()> {
    let closing = CancellationToken::new();
    backend::request(json!({"operation":"inspect"}), &closing)
        .await
        .map_err(|_| ())?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| ())?;
    let host = listener.local_addr().map_err(|_| ())?.to_string();
    let origin = format!("http://{host}");
    let bootstrap = token().map_err(|_| ())?;
    let (activity, mut activity_rx) = watch::channel(());
    let state = State {
        host,
        origin: origin.clone(),
        tokens: Arc::new(Mutex::new(Tokens {
            bootstrap: Some(bootstrap.clone()),
            session: None,
        })),
        closing: closing.clone(),
        active: Arc::new(Semaphore::new(4)),
        activity,
    };
    let url = format!("{origin}/#{bootstrap}");
    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| ())?;
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ())?;
    #[cfg(unix)]
    let mut hangup =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).map_err(|_| ())?;
    let signal = async {
        #[cfg(unix)]
        {
            tokio::select! { _ = interrupt.recv() => 130, _ = terminate.recv() => 143, _ = hangup.recv() => 129 }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
            130
        }
    };
    tokio::pin!(signal);
    if json_out {
        crate::cli::write_json(&json!({"url":url,"idleTimeoutSeconds":idle_timeout.as_secs()}));
    } else {
        println!(
            "Octocode config view: {url}\nPress Ctrl+C to close. This temporary URL grants access to your configuration."
        );
    }
    let opener = async {
        if no_open {
            Ok(())
        } else {
            open_browser(&url).await
        }
    };
    tokio::pin!(opener);
    let mut opened = false;
    let mut connections = JoinSet::new();
    let idle = tokio::time::sleep(idle_timeout);
    tokio::pin!(idle);
    let code = loop {
        tokio::select! {
            _ = closing.cancelled() => break 0,
            code = &mut signal => break code,
            _ = &mut idle => break 0,
            result = &mut opener, if !opened => {
                opened = true;
                if result.is_err() { break 5; }
            },
            changed = activity_rx.changed() => if changed.is_ok() {
                idle.as_mut().reset(tokio::time::Instant::now() + idle_timeout);
            },
            _ = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    if connections.len() >= MAX_CONNECTIONS { continue; }
                    let state = state.clone();
                    connections.spawn(async move {
                        let shutdown = state.closing.clone();
                        let service = hyper::service::service_fn(move |request| {
                            let state = state.clone();
                            async move { Ok::<_, Infallible>(handle(request, state).await) }
                        });
                        let mut builder = http1::Builder::new();
                        builder.timer(TokioTimer::new()).header_read_timeout(HEADER_TIMEOUT)
                            .max_headers(30).max_buf_size(16 * 1024);
                        let connection = builder.serve_connection(TokioIo::new(stream), service);
                        tokio::pin!(connection);
                        tokio::select! {
                            _ = &mut connection => {},
                            _ = shutdown.cancelled() => {
                                connection.as_mut().graceful_shutdown();
                                let _ = tokio::time::timeout(DRAIN_TIMEOUT, &mut connection).await;
                            },
                        }
                    });
                },
                Err(_) => break 5,
            },
        }
    };
    drop(listener);
    closing.cancel();
    {
        let mut tokens = state
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tokens.bootstrap = None;
        tokens.session = None;
    }
    if tokio::time::timeout(DRAIN_TIMEOUT, async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        connections.shutdown().await;
    }
    if code == 5 { Err(()) } else { Ok(code) }
}

async fn open_browser(url: &str) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    let mut child = command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        _ => {
            let _ = child.kill().await;
            Err(io::Error::other("browser opener failed"))
        }
    }
}
