//! The bounded stderr ring and its capped line reader.

use super::*;
use crate::lsp::client::stderr::{
    CancelRead, Overlapped, READ_CANCELLED, ReadNow, STDERR_LINE_MAX_CHARS, STDERR_RING_CAPACITY,
    UntilExit, drain_stderr, push_stderr_line, read_capped_line,
};
use tokio::io::BufReader;

#[test]
fn stderr_ring_keeps_only_recent_lines() {
    let lines = Arc::new(StdMutex::new(VecDeque::new()));

    for index in 0..(STDERR_RING_CAPACITY + 5) {
        push_stderr_line(&lines, format!("line-{index}"));
    }

    let lines = lines.lock().expect("stderr ring lock");
    assert_eq!(lines.len(), STDERR_RING_CAPACITY);
    assert_eq!(lines.front().map(String::as_str), Some("line-5"));
    assert_eq!(
        lines.back().map(String::as_str),
        Some(format!("line-{}", STDERR_RING_CAPACITY + 4).as_str())
    );
}

#[test]
fn stderr_ring_truncates_very_long_lines() {
    let lines = Arc::new(StdMutex::new(VecDeque::new()));

    push_stderr_line(&lines, "x".repeat(STDERR_LINE_MAX_CHARS + 10));

    let line = lines
        .lock()
        .expect("stderr ring lock")
        .front()
        .cloned()
        .expect("stderr line");
    assert_eq!(line.chars().count(), STDERR_LINE_MAX_CHARS + 3);
    assert!(line.ends_with("..."));
}

#[test]
fn stderr_drain_bounds_a_giant_line_and_keeps_reading() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut input = vec![b'x'; 4 * 1024 * 1024];
        input.extend_from_slice(b"\r\nnext line\n");
        let lines = Arc::new(StdMutex::new(VecDeque::new()));
        drain_stderr(input.as_slice(), Arc::clone(&lines)).await;
        let lines = lines.lock().unwrap();
        assert_eq!(
            lines.len(),
            2,
            "{:?}",
            lines.iter().map(String::len).collect::<Vec<_>>()
        );
        assert!(lines[0].len() <= STDERR_LINE_MAX_CHARS + 3);
        assert!(lines[0].ends_with("..."));
        assert_eq!(lines[1], "next line");
    });
}

#[test]
fn stderr_drain_survives_invalid_utf8() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let input: &[u8] = b"bad \xff\xfe bytes\nstill draining\nlast without newline";
        let lines = Arc::new(StdMutex::new(VecDeque::new()));
        drain_stderr(input, Arc::clone(&lines)).await;
        let lines = lines.lock().unwrap();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("bad ") && lines[0].contains('\u{FFFD}'));
        assert_eq!(lines[1], "still draining");
        assert_eq!(lines[2], "last without newline");
    });
}

#[test]
fn capped_line_reader_discards_the_overflow_without_buffering_it() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let input = [vec![b'a'; 100], b"\nb\n".to_vec()].concat();
        let mut reader = BufReader::with_capacity(16, input.as_slice());
        let mut line = Vec::new();
        let (consumed, truncated) = read_capped_line(&mut reader, &mut line, 10).await.unwrap();
        assert_eq!((consumed, truncated, line.len()), (101, true, 10));
        line.clear();
        let (consumed, truncated) = read_capped_line(&mut reader, &mut line, 10).await.unwrap();
        assert_eq!(
            (consumed, truncated, line.as_slice()),
            (2, false, &b"b"[..])
        );
        line.clear();
        assert_eq!(
            read_capped_line(&mut reader, &mut line, 10)
                .await
                .unwrap()
                .0,
            0
        );
    });
}

/// A pipe a leftover process holds open: its stream never answers, and it
/// holds `held` unread (read without waiting, then `WouldBlock`), as a Unix
/// pipe read with a non-blocking `read`.
struct HeldPipe {
    held: Vec<u8>,
}

impl tokio::io::AsyncRead for HeldPipe {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

impl ReadNow for HeldPipe {
    fn poll_read_now(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let read = buf.remaining().min(self.held.len());
        buf.put_slice(&self.held[..read]);
        self.held.drain(..read);
        std::task::Poll::Ready(Ok(()))
    }
}

/// Once the server has exited, the drain keeps every line the pipe holds,
/// a last one without its newline too, and ends without waiting for EOF.
#[tokio::test]
async fn an_exited_servers_drain_reads_what_the_pipe_holds_and_ends() {
    let (exited, on_exit) = tokio::sync::oneshot::channel();
    let pipe = HeldPipe {
        held: b"first\nsecond\nno newline".to_vec(),
    };
    let lines = Arc::new(StdMutex::new(VecDeque::new()));
    let drain = tokio::spawn(drain_stderr(
        UntilExit::new(pipe, on_exit),
        Arc::clone(&lines),
    ));
    exited.send(()).expect("drain listens");
    drain.await.expect("drain ends");
    let lines = lines.lock().unwrap();
    assert_eq!(
        lines.iter().map(String::as_str).collect::<Vec<_>>(),
        ["first", "second", "no newline"]
    );
}

/// The reactor's read in flight on an overlapped pipe (Windows).
enum InFlight {
    /// Parked on an empty pipe: only a writer or a cancel completes it.
    Parked,
    /// Completed with the bytes it took, or cancelled (`None`). `queued`
    /// while its completion has not yet reached the stream.
    Done { took: Option<Vec<u8>>, queued: bool },
}

/// An overlapped pipe (Windows) that a leftover writer holds open, so it
/// never reaches EOF. A read is always in flight: it takes the next chunk as
/// soon as the pipe holds one, and the stream hands it over one reactor turn
/// later. A read that took the last chunk therefore looks, for that turn,
/// like a read parked on an empty pipe.
struct OverlappedPipe {
    held: VecDeque<Vec<u8>>,
    read: InFlight,
}

impl OverlappedPipe {
    fn new(held: &[&[u8]]) -> Self {
        let mut pipe = Self {
            held: held.iter().map(|chunk| chunk.to_vec()).collect(),
            read: InFlight::Parked,
        };
        pipe.schedule_read();
        pipe
    }

    fn schedule_read(&mut self) {
        self.read = match self.held.pop_front() {
            Some(chunk) => InFlight::Done {
                took: Some(chunk),
                queued: true,
            },
            None => InFlight::Parked,
        };
    }
}

impl tokio::io::AsyncRead for OverlappedPipe {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.read {
            InFlight::Parked => std::task::Poll::Pending,
            InFlight::Done { queued, .. } if *queued => {
                *queued = false;
                std::task::Poll::Pending
            }
            InFlight::Done { took, .. } => {
                let took = took.take();
                self.schedule_read();
                std::task::Poll::Ready(match took {
                    Some(chunk) => {
                        buf.put_slice(&chunk);
                        Ok(())
                    }
                    None => Err(std::io::Error::from_raw_os_error(READ_CANCELLED)),
                })
            }
        }
    }
}

impl CancelRead for OverlappedPipe {
    fn cancel_read(&mut self) {
        if matches!(self.read, InFlight::Parked) {
            self.read = InFlight::Done {
                took: None,
                queued: true,
            };
        }
    }
}

/// Once the server has exited, the drain of an overlapped pipe keeps the
/// chunk its read in flight took while the pipe looked empty, and it ends on
/// the cancelled read while a leftover writer still holds the pipe open.
#[test]
fn an_exited_servers_overlapped_drain_keeps_the_read_in_flight_and_ends() {
    let pipe = Overlapped(OverlappedPipe::new(&[b"first\nsec", b"ond\nlast"]));
    let (exited, on_exit) = tokio::sync::oneshot::channel();
    let lines = Arc::new(StdMutex::new(VecDeque::new()));
    let mut drain = std::pin::pin!(drain_stderr(
        UntilExit::new(pipe, on_exit),
        Arc::clone(&lines),
    ));
    exited.send(()).expect("drain listens");
    // Each poll is one reactor turn. A drain that waits for a writer never
    // ends; one that ends on the empty-looking pipe loses "last".
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let turns = (1..=64).find(|_| drain.as_mut().poll(&mut cx).is_ready());
    assert!(turns.is_some(), "the drain waits for the pipe's writer");
    let lines = lines.lock().unwrap();
    assert_eq!(
        lines.iter().map(String::as_str).collect::<Vec<_>>(),
        ["first", "second", "last"]
    );
}

/// A running server's drain (no exit signal ever sent) reads to EOF: the
/// signal's sender going away does not end it.
#[tokio::test]
async fn a_running_servers_drain_reads_to_eof() {
    struct Stream(tokio::io::DuplexStream);
    impl tokio::io::AsyncRead for Stream {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
        }
    }
    impl ReadNow for Stream {
        fn poll_read_now(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }
    let (mut writer, reader) = tokio::io::duplex(64);
    let (exited, on_exit) = tokio::sync::oneshot::channel::<()>();
    let lines = Arc::new(StdMutex::new(VecDeque::new()));
    let drain = tokio::spawn(drain_stderr(
        UntilExit::new(Stream(reader), on_exit),
        Arc::clone(&lines),
    ));
    tokio::io::AsyncWriteExt::write_all(&mut writer, b"before\n")
        .await
        .unwrap();
    drop(exited);
    tokio::io::AsyncWriteExt::write_all(&mut writer, b"after\n")
        .await
        .unwrap();
    drop(writer);
    drain.await.expect("drain ends at EOF");
    let lines = lines.lock().unwrap();
    assert_eq!(
        lines.iter().map(String::as_str).collect::<Vec<_>>(),
        ["before", "after"]
    );
}

/// A server that dies at launch says why on stderr; the start error carries it
/// instead of only "connection closed".
#[tokio::test]
#[cfg(unix)]
async fn a_failed_start_reports_the_servers_stderr() {
    let tmp = TmpDir::new();
    let script = tmp.0.join("dying-server");
    write_executable(
        &script,
        "#!/bin/sh\necho 'error: relative path toolchain' >&2\nexit 1\n",
    );
    let mut config = ts_config();
    config.command = script.to_string_lossy().into_owned();
    config.args = None;
    let client = NativeLspClient::new(config);
    let error = client
        .start()
        .await
        .expect_err("the server exits at launch");
    assert!(
        error.reason.contains("error: relative path toolchain"),
        "{}",
        error.reason
    );
}

/// A server that dies at launch leaves a detached process (its own session,
/// so the group sweep misses it) holding the stderr pipe open: the start
/// still returns, with the line the server wrote, while that process lives.
#[tokio::test]
#[cfg(unix)]
async fn a_failed_start_returns_while_a_detached_process_holds_stderr() {
    if !node_available() {
        return;
    }
    let tmp = TmpDir::new();
    let script = tmp.0.join("dying-server.js");
    let holder_file = tmp.0.join("holder.pid");
    write_executable(
        &script,
        r#"#!/usr/bin/env node
const { spawn } = require('child_process');
const fs = require('fs');
const holder = spawn(process.execPath, ['-e', 'setTimeout(() => {}, 60000)'], { stdio: ['ignore', 'ignore', 'inherit'], detached: true });
holder.unref();
fs.writeFileSync(process.env.FAKE_HOLDER_PID_FILE, String(holder.pid));
fs.writeSync(2, 'error: the server could not start\n');
process.exit(1);
"#,
    );
    let mut config = ts_config();
    config.command = script.to_string_lossy().into_owned();
    config.args = None;
    config.env = Some(
        [
            (
                "FAKE_HOLDER_PID_FILE".to_owned(),
                holder_file.to_string_lossy().into_owned(),
            ),
            ("PATH".to_owned(), std::env::var("PATH").unwrap_or_default()),
        ]
        .into_iter()
        .collect(),
    );
    let client = NativeLspClient::new(config);
    let error = client
        .start()
        .await
        .expect_err("the server exits at launch");
    let holder: i32 = std::fs::read_to_string(&holder_file)
        .expect("holder pid")
        .trim()
        .parse()
        .expect("numeric pid");
    // SAFETY: signal 0 only probes the pid the fixture recorded.
    let held = unsafe { libc::kill(holder, 0) } == 0;
    // SAFETY: the fixture recorded this holder's pid; end it so the test
    // leaves nothing behind.
    unsafe {
        libc::kill(holder, libc::SIGKILL);
    }
    assert!(
        held,
        "the start returned only after the holder {holder} exited"
    );
    assert!(
        error.reason.contains("error: the server could not start"),
        "{}",
        error.reason
    );
}
