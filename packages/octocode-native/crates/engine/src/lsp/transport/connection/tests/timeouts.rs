use super::*;

#[tokio::test(start_paused = true)]
async fn request_timeout_bounds_a_blocked_write() {
    let (client_w, _server_r) = duplex(1);
    let (_server_w, client_r) = duplex(8192);
    let connection =
        JsonRpcConnection::new(client_r, client_w, context(), ProgressTracker::new(), None);
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

/// One slow request is cancelled with `$/cancelRequest` and the server is
/// kept: the next request on the same connection is answered, with no
/// retirement (and so no cold restart by the pool).
#[tokio::test(start_paused = true)]
async fn timed_out_request_sends_cancel_and_keeps_the_connection() {
    let (connection, mut server) = connect();
    let error = connection
        .request("textDocument/references", Value::Null, 50)
        .await
        .expect_err("server never answers in time");
    assert_eq!(error.kind(), &ErrorKind::Timeout);
    assert!(error.reason.contains("timed out"));
    assert!(
        connection.is_alive(),
        "one slow request does not retire the server"
    );
    assert_eq!(pending_len(&connection), Some(0), "entry freed on timeout");
    assert_eq!(server.read_message().await["id"], 1);
    let cancel = server.read_message().await;
    assert_eq!(cancel["method"], "$/cancelRequest");
    assert_eq!(cancel["params"]["id"], 1);
    // The server acknowledges the cancel late; the next request is answered.
    server
        .send(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32800,"message":"cancelled"}}))
        .await;
    let answer = tokio::spawn(async move {
        let request = server.read_message().await;
        assert_eq!(request["method"], "textDocument/definition");
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"result":[]}))
            .await;
        server
    });
    assert_eq!(
        connection
            .request("textDocument/definition", Value::Null, 5_000)
            .await
            .expect("same server answers the next request"),
        json!([])
    );
    let _server = answer.await;
    assert!(connection.is_alive());
}

/// An answered request between two timeouts resets the count: only
/// back-to-back timeouts retire the server.
#[tokio::test(start_paused = true)]
async fn an_answered_request_resets_the_timeout_count() {
    let (connection, mut server) = connect();
    let connection = Arc::new(connection);
    assert!(connection.request("slow", Value::Null, 50).await.is_err());
    assert_eq!(server.read_message().await["method"], "slow");
    assert_eq!(server.read_message().await["method"], "$/cancelRequest");
    let answered = tokio::spawn({
        let connection = Arc::clone(&connection);
        async move { connection.request("quick", Value::Null, 5_000).await }
    });
    let request = server.read_message().await;
    server
        .send(json!({"jsonrpc":"2.0","id":request["id"],"result":"ok"}))
        .await;
    assert_eq!(answered.await.expect("join").expect("quick"), json!("ok"));
    assert!(connection.request("slow", Value::Null, 50).await.is_err());
    assert!(
        connection.is_alive(),
        "a timeout after an answered request is again the first"
    );
}

#[tokio::test(start_paused = true)]
async fn second_consecutive_timeout_retires_the_connection() {
    let (connection, mut server) = connect();
    for _ in 0..RETIRE_AFTER_TIMEOUTS {
        let error = connection
            .request("textDocument/definition", Value::Null, 50)
            .await
            .expect_err("server never answers");
        assert_eq!(error.kind(), &ErrorKind::Timeout);
    }
    assert!(
        !connection.is_alive(),
        "a server that answers nothing across timeouts is retired as wedged"
    );
    for id in 1..=u64::from(RETIRE_AFTER_TIMEOUTS) {
        assert_eq!(server.read_message().await["id"], id);
        let cancel = server.read_message().await;
        assert_eq!(cancel["method"], "$/cancelRequest");
        assert_eq!(cancel["params"]["id"], id);
    }
}

#[tokio::test(start_paused = true)]
async fn retirement_fails_all_pending_immediately() {
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
    assert!(connection.is_alive(), "first timeout keeps the server");
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
