use super::*;

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
        None,
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
                crate::lsp::transport::codec::MAX_CONTENT_LENGTH + 1
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
    for _ in 0..RETIRE_AFTER_TIMEOUTS {
        assert!(
            connection
                .request("shutdown", Value::Null, 50)
                .await
                .is_err()
        );
    }
    assert!(!connection.is_alive());
    assert!(
        connection.notify("x", Value::Null).await.is_err(),
        "failed connection refuses notify"
    );
    connection
        .notify_best_effort("exit", Value::Null)
        .await
        .expect("exit written even after the timeout");
    for _ in 0..RETIRE_AFTER_TIMEOUTS {
        assert_eq!(server.read_message().await["method"], "shutdown");
        assert_eq!(server.read_message().await["method"], "$/cancelRequest");
    }
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
        None,
    );
    let error = connection
        .notify("initialized", json!({}))
        .await
        .expect_err("write cannot complete");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert!(!connection.is_alive());
}
