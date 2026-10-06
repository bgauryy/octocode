use super::super::codec::{Frame, encode_frame, read_frame};
use super::*;
use crate::error::{ErrorCode, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufReadExt, DuplexStream, ReadBuf, duplex};

/// The fake server's two pipe ends.
struct Server {
    reader: BufReader<DuplexStream>,
    writer: DuplexStream,
}

impl Server {
    async fn read_message(&mut self) -> Value {
        match read_frame(&mut self.reader).await {
            Ok(Frame::Body(body)) => serde_json::from_slice(&body).expect("client sent valid JSON"),
            other => panic!("expected a client frame, got {other:?}"),
        }
    }

    async fn send(&mut self, message: Value) {
        let frame = encode_frame(&message).expect("encode");
        self.writer.write_all(&frame).await.expect("server write");
        self.writer.flush().await.expect("server flush");
    }

    async fn send_raw(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).await.expect("server write");
    }
}

fn context() -> ClientRequestContext {
    ClientRequestContext {
        configuration: json!({"gopls": {"a": 1}}),
        section_root: None,
        workspace_folders: json!([{ "uri": "file:///w", "name": "workspace" }]),
    }
}

fn connect_with_buffer(client_to_server: usize) -> (JsonRpcConnection, Server) {
    let (client_w, server_r) = duplex(client_to_server);
    let (server_w, client_r) = duplex(64 * 1024);
    let connection = JsonRpcConnection::new(client_r, client_w, context(), ProgressTracker::new());
    (
        connection,
        Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        },
    )
}

fn connect() -> (JsonRpcConnection, Server) {
    connect_with_buffer(64 * 1024)
}

fn pending_len(connection: &JsonRpcConnection) -> Option<usize> {
    connection
        .shared
        .pending
        .lock()
        .expect("pending lock")
        .as_ref()
        .map(HashMap::len)
}

async fn wait_until_dead(connection: &JsonRpcConnection) {
    for _ in 0..100 {
        if !connection.is_alive() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("connection should be marked dead");
}

/// A writer whose pipe never drains: models a server that stopped reading stdin.
struct StalledWriter;

impl AsyncWrite for StalledWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Pending
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

/// A reader that never yields bytes.
struct SilentReader;

impl AsyncRead for SilentReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

#[test]
fn notify_write_deadline_scales_with_payload_size() {
    assert_eq!(notify_write_deadline_ms(0), NOTIFY_BASE_DEADLINE_MS);
    assert_eq!(notify_write_deadline_ms(1_024), NOTIFY_BASE_DEADLINE_MS);
    assert_eq!(
        notify_write_deadline_ms(3 * 1024 * 1024),
        NOTIFY_BASE_DEADLINE_MS + 3 * NOTIFY_MS_PER_MIB
    );
    assert!(notify_write_deadline_ms(8 * 1024 * 1024) > notify_write_deadline_ms(1024 * 1024));
}

#[tokio::test]
async fn request_with_partials_collects_progress_before_final_response() {
    let (connection, mut server) = connect();
    let server_task = tokio::spawn(async move {
        let request = server.read_message().await;
        let token = request["params"]["partialResultToken"].clone();
        let id = request["id"].clone();
        server
            .send(json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":token,"value":[{"uri":"partial"}]}}))
            .await;
        server
            .send(json!({"jsonrpc":"2.0","id":id,"result":[{"uri":"final"}]}))
            .await;
        server
    });
    let result = connection
        .request_with_partials("textDocument/references", json!({}), 1_000)
        .await
        .expect("partial request");
    let _server = server_task.await.expect("server");
    assert_eq!(result, json!([{"uri":"partial"},{"uri":"final"}]));
    assert_eq!(connection.shared.partial_results.len(), 0);
}

#[tokio::test]
async fn partial_chunks_with_a_kind_field_are_results_not_progress() {
    let (connection, mut server) = connect();
    let server_task = tokio::spawn(async move {
        let request = server.read_message().await;
        let token = request["params"]["partialResultToken"].clone();
        server
            .send(json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":token,"value":{"kind":"full","items":[1]}}}))
            .await;
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"result":{"kind":"full","items":[2]}}))
            .await;
        server
    });
    let result = connection
        .request_with_partials("textDocument/diagnostic", json!({}), 1_000)
        .await
        .expect("partial request");
    let _server = server_task.await.expect("server");
    assert_eq!(result, json!({"kind":"full","items":[1,2]}));
}

#[tokio::test(start_paused = true)]
async fn dropped_partial_request_releases_its_token_buffer() {
    let (connection, mut server) = connect();
    let dropped = tokio::time::timeout(
        Duration::from_millis(10),
        connection.request_with_partials("textDocument/references", json!({}), 5_000),
    )
    .await;
    assert!(dropped.is_err(), "outer timeout drops the request future");
    let request = server.read_message().await;
    assert_eq!(connection.shared.partial_results.len(), 0);
    // A late chunk for the released token is ignored, not buffered.
    server
        .send(json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":request["params"]["partialResultToken"],"value":[1]}}))
        .await;
    tokio::task::yield_now().await;
    assert_eq!(connection.shared.partial_results.len(), 0);
}

#[tokio::test]
async fn rust_analyzer_server_status_feeds_progress_readiness() {
    let (client_w, server_r) = duplex(64 * 1024);
    let (server_w, client_r) = duplex(64 * 1024);
    let progress = ProgressTracker::new();
    let _connection = JsonRpcConnection::new(client_r, client_w, context(), Arc::clone(&progress));
    let mut server = Server {
        reader: BufReader::new(server_r),
        writer: server_w,
    };
    let mut active = progress.subscribe();
    server
        .send(json!({"jsonrpc":"2.0","method":"experimental/serverStatus","params":{"health":"ok","quiescent":false}}))
        .await;
    tokio::time::timeout(Duration::from_secs(5), active.wait_for(|count| *count == 1))
        .await
        .expect("non-quiescent status opens a progress token")
        .expect("tracker alive");
    server
        .send(json!({"jsonrpc":"2.0","method":"experimental/serverStatus","params":{"health":"ok","quiescent":true}}))
        .await;
    tokio::time::timeout(Duration::from_secs(5), active.wait_for(|count| *count == 0))
        .await
        .expect("quiescent status closes it")
        .expect("tracker alive");
}

#[tokio::test]
async fn connection_is_alive_until_the_server_stream_closes() {
    let (connection, server) = connect();
    assert!(connection.is_alive());
    drop(server);
    wait_until_dead(&connection).await;
}

#[tokio::test]
async fn connection_caches_push_diagnostics_for_bounded_on_demand_reads() {
    let (connection, mut server) = connect();
    server
        .send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///workspace/a.ts",
                "version": 3,
                "diagnostics": [{ "message": "broken" }]
            }
        }))
        .await;
    let report = connection
        .wait_for_push_diagnostics("file:///workspace/a.ts", 1_000, Some(3))
        .await
        .expect("push diagnostics report");
    assert_eq!(report["kind"], "full");
    assert_eq!(report["version"], 3);
    assert_eq!(report["items"][0]["message"], "broken");

    connection.clear_push_diagnostics("file:///workspace/a.ts");
    assert!(
        connection
            .wait_for_push_diagnostics("file:///workspace/a.ts", 1, Some(3))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn lengthless_frame_is_skipped_and_string_ids_route() {
    let (connection, mut server) = connect();
    let server_task = tokio::spawn(async move {
        let request = server.read_message().await;
        server.send_raw(b"\r\n").await;
        let id = request["id"].as_u64().expect("integer id").to_string();
        server
            .send(json!({"jsonrpc":"2.0","id":id,"result":{"ok":true}}))
            .await;
        server
    });
    let value = connection
        .request("x", Value::Null, 1_000)
        .await
        .expect("response routed");
    assert_eq!(value, json!({"ok": true}));
    let _server = server_task.await;
    assert!(connection.is_alive());
}

#[tokio::test]
async fn concurrent_requests_resolve_out_of_order() {
    let (connection, mut server) = connect();
    let connection = Arc::new(connection);
    let first = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("a", Value::Null, 5_000).await }
    });
    let second = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("b", Value::Null, 5_000).await }
    });
    let one = server.read_message().await;
    let two = server.read_message().await;
    for request in [two, one] {
        let method = request["method"].clone();
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"result":method}))
            .await;
    }
    assert_eq!(first.await.expect("join").expect("a"), json!("a"));
    assert_eq!(second.await.expect("join").expect("b"), json!("b"));
}

#[tokio::test]
async fn rpc_errors_are_typed_through_the_connection() {
    let (connection, mut server) = connect();
    let server_task = tokio::spawn(async move {
        let request = server.read_message().await;
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32801,"message":"content modified"}}))
            .await;
        server
    });
    let error = connection
        .request("x", Value::Null, 1_000)
        .await
        .expect_err("rpc error");
    let _server = server_task.await;
    assert_eq!(
        error.rpc_error().map(|error| error.code),
        Some(ErrorCode::ContentModified)
    );
    assert!(error.reason.contains("content modified"));
    assert!(
        connection.is_alive(),
        "an RPC error is not a transport fault"
    );
}

#[tokio::test(start_paused = true)]
async fn request_timeout_bounds_a_blocked_write() {
    let (client_w, _server_r) = duplex(1);
    let (_server_w, client_r) = duplex(8192);
    let connection = JsonRpcConnection::new(client_r, client_w, context(), ProgressTracker::new());
    let error = connection
        .request("blocked", Value::Null, 20)
        .await
        .expect_err("write cannot complete");
    assert!(error.reason.contains("timed out"), "{}", error.reason);
    assert_eq!(
        pending_len(&connection),
        None,
        "pending map taken on failure"
    );
    assert!(
        !connection.is_alive(),
        "a partial frame cannot be safely reused"
    );
}

#[tokio::test(start_paused = true)]
async fn request_timeout_bounds_a_blocked_cancellation() {
    let frame_size =
        encode_frame(&json!({"jsonrpc":"2.0","id":1,"method":"blocked","params":null}))
            .expect("encode")
            .len();
    let (connection, _server) = connect_with_buffer(frame_size);
    let started = Instant::now();
    let error = connection
        .request("blocked", Value::Null, 20)
        .await
        .expect_err("response timed out");
    assert!(error.reason.contains("timed out"));
    assert!(started.elapsed() <= Duration::from_millis(20 + CANCEL_WRITE_WAIT_MS));
    assert!(!connection.is_alive());
}

#[tokio::test(start_paused = true)]
async fn timed_out_request_sends_cancel_and_retires_the_connection() {
    let (connection, mut server) = connect();
    let error = connection
        .request("textDocument/definition", Value::Null, 50)
        .await
        .expect_err("server never answers");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert!(error.reason.contains("timed out"));
    assert!(
        !connection.is_alive(),
        "a wedged connection is retired for eviction"
    );
    assert_eq!(server.read_message().await["id"], 1);
    let cancel = server.read_message().await;
    assert_eq!(cancel["method"], "$/cancelRequest");
    assert_eq!(cancel["params"]["id"], 1);
}

#[tokio::test(start_paused = true)]
async fn timeout_fails_all_pending_immediately() {
    let (connection, _server) = connect();
    let connection = Arc::new(connection);
    let started = Instant::now();
    let slow = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move {
            let result = connection.request("slow", Value::Null, 60_000).await;
            (result, Instant::now())
        }
    });
    tokio::task::yield_now().await;
    let _ = connection.request("fast", Value::Null, 100).await;
    let (result, finished) = slow.await.expect("join");
    let error = result.expect_err("other waiters fail with the connection");
    assert_eq!(error.kind(), &ErrorKind::ConnectionClosed);
    assert!(error.reason.contains("retired"), "{}", error.reason);
    assert!(
        finished.duration_since(started) < Duration::from_secs(1),
        "the slow waiter must not sit out its own 60s timeout"
    );
    assert!(connection.request("after", Value::Null, 100).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn dropped_request_sends_cancel_and_frees_its_entry() {
    let (connection, mut server) = connect();
    let dropped = tokio::time::timeout(
        Duration::from_millis(10),
        connection.request("slow", Value::Null, 5_000),
    )
    .await;
    assert!(dropped.is_err());
    assert_eq!(pending_len(&connection), Some(0), "entry freed on drop");
    assert!(
        connection.is_alive(),
        "cancellation by the caller is not a fault"
    );
    assert_eq!(server.read_message().await["id"], 1);
    let cancel = server.read_message().await;
    assert_eq!(cancel["method"], "$/cancelRequest");
    assert_eq!(cancel["params"]["id"], 1);
    // A late response for the cancelled id is ignored; the next request works.
    server
        .send(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32800,"message":"cancelled"}}))
        .await;
    let next = tokio::spawn(async move {
        let request = server.read_message().await;
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"result":"ok"}))
            .await;
        server
    });
    assert_eq!(
        connection
            .request("next", Value::Null, 5_000)
            .await
            .expect("next"),
        json!("ok")
    );
    let _server = next.await;
}

#[tokio::test]
async fn dropped_mid_write_cannot_corrupt_the_stream() {
    // A 16-byte pipe forces the writer to block mid-frame; the caller's future
    // is dropped while the frame is half on the wire.
    let (connection, mut server) = connect_with_buffer(16);
    let connection = Arc::new(connection);
    let big = "x".repeat(8 * 1024);
    let request = tokio::spawn({
        let connection = Arc::clone(&connection);
        let big = big.clone();
        async move {
            connection
                .request("big", json!({ "text": big }), 5_000)
                .await
        }
    });
    // Peek (without consuming) until the writer has put part of the frame on
    // the wire and is blocked on the full pipe.
    let started = server.reader.fill_buf().await.expect("frame started").len();
    assert!(started > 0);
    request.abort();
    let _ = request.await;
    // Everything that follows still decodes frame by frame.
    let mut rest = server.read_message().await;
    assert_eq!(
        rest["params"]["text"].take(),
        json!(big),
        "frame written whole"
    );
    let cancel = server.read_message().await;
    assert_eq!(cancel["method"], "$/cancelRequest");
    assert!(connection.is_alive());
    let next = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("next", Value::Null, 5_000).await }
    });
    let request = server.read_message().await;
    assert_eq!(request["method"], "next");
    server
        .send(json!({"jsonrpc":"2.0","id":request["id"],"result":1}))
        .await;
    assert_eq!(next.await.expect("join").expect("next"), json!(1));
}

#[tokio::test]
async fn server_request_is_answered_per_section_via_the_writer() {
    let (connection, mut server) = connect();
    server
        .send(json!({
            "jsonrpc":"2.0","id":"cfg-1","method":"workspace/configuration",
            "params":{"items":[{"section":"gopls"},{"section":"unknown"}]}
        }))
        .await;
    let reply = server.read_message().await;
    assert_eq!(reply["id"], "cfg-1");
    assert_eq!(reply["result"], json!([{"a": 1}, null]));
    server
        .send(json!({"jsonrpc":"2.0","id":7,"method":"custom/unknown"}))
        .await;
    assert_eq!(server.read_message().await["error"]["code"], -32601);
    assert!(connection.is_alive());
}

#[tokio::test]
async fn server_request_reply_does_not_block_reads_when_server_stops_reading() {
    let (server_w, client_r) = duplex(64 * 1024);
    let mut server_w = server_w;
    let connection = Arc::new(JsonRpcConnection::new(
        client_r,
        StalledWriter,
        context(),
        ProgressTracker::new(),
    ));
    let request = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("a", Value::Null, 5_000).await }
    });
    while pending_len(&connection) != Some(1) {
        tokio::task::yield_now().await;
    }
    for message in [
        json!({"jsonrpc":"2.0","id":"s1","method":"workspace/configuration","params":{"items":[{}]}}),
        json!({"jsonrpc":"2.0","id":1,"result":"read despite a stalled stdin"}),
    ] {
        server_w
            .write_all(&encode_frame(&message).expect("encode"))
            .await
            .expect("server write");
    }
    let result = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("reader must not block on the reply write")
        .expect("join");
    assert_eq!(
        result.expect("response"),
        json!("read despite a stalled stdin")
    );
}

#[tokio::test]
async fn garbage_content_length_fails_every_pending_request() {
    let (connection, mut server) = connect();
    let connection = Arc::new(connection);
    let request = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("a", Value::Null, 5_000).await }
    });
    let _ = server.read_message().await;
    server.send_raw(b"Content-Length: abc\r\n\r\n{}").await;
    let error = request.await.expect("join").expect_err("fatal framing");
    assert!(
        error.reason.contains("invalid Content-Length"),
        "{}",
        error.reason
    );
    assert!(!connection.is_alive());
}

#[tokio::test]
async fn oversized_frame_fails_pending_and_marks_dead() {
    let (connection, mut server) = connect();
    let connection = Arc::new(connection);
    let request = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("a", Value::Null, 5_000).await }
    });
    let _ = server.read_message().await;
    server
        .send_raw(
            format!(
                "Content-Length: {}\r\n\r\n",
                super::super::codec::MAX_CONTENT_LENGTH + 1
            )
            .as_bytes(),
        )
        .await;
    let error = request.await.expect("join").expect_err("oversized frame");
    assert!(error.reason.contains("maximum JSON-RPC frame size"));
    assert!(!connection.is_alive());
}

#[tokio::test]
async fn unparseable_frames_are_counted_and_reported() {
    let (connection, mut server) = connect();
    let connection = Arc::new(connection);
    let request = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("a", Value::Null, 5_000).await }
    });
    let _ = server.read_message().await;
    server.send_raw(b"Content-Length: 3\r\n\r\nabc").await;
    server.send_raw(b"Content-Length: 2\r\n\r\n[]").await;
    drop(server);
    let error = request.await.expect("join").expect_err("closed");
    assert_eq!(connection.unparseable_frames(), 2);
    assert!(
        error.reason.contains("2 unparseable frame(s)"),
        "{}",
        error.reason
    );
}

#[tokio::test(start_paused = true)]
async fn exit_is_still_sent_after_a_timed_out_shutdown() {
    let (connection, mut server) = connect();
    assert!(
        connection
            .request("shutdown", Value::Null, 50)
            .await
            .is_err()
    );
    assert!(!connection.is_alive());
    assert!(
        connection.notify("x", Value::Null).await.is_err(),
        "failed connection refuses notify"
    );
    connection
        .notify_best_effort("exit", Value::Null)
        .await
        .expect("exit written even after the timeout");
    assert_eq!(server.read_message().await["method"], "shutdown");
    assert_eq!(server.read_message().await["method"], "$/cancelRequest");
    assert_eq!(server.read_message().await["method"], "exit");
}

#[tokio::test]
async fn dropping_the_connection_aborts_both_tasks() {
    let (connection, mut server) = connect();
    let shared = Arc::clone(&connection.shared);
    drop(connection);
    // Writer task aborted → the server sees stdin close.
    assert!(matches!(
        read_frame(&mut server.reader).await,
        Ok(Frame::Eof)
    ));
    // FailOnExit ran on abort.
    for _ in 0..100 {
        if !shared.is_alive() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(!shared.is_alive());
    assert_eq!(
        Arc::strong_count(&shared),
        1,
        "tasks released their handles"
    );
}

#[tokio::test(start_paused = true)]
async fn notify_times_out_when_the_server_stops_reading() {
    let connection = JsonRpcConnection::new(
        SilentReader,
        StalledWriter,
        context(),
        ProgressTracker::new(),
    );
    let error = connection
        .notify("initialized", json!({}))
        .await
        .expect_err("write cannot complete");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert!(!connection.is_alive());
}
