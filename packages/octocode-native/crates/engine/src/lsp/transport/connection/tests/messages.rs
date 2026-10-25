use super::*;

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

#[tokio::test(start_paused = true)]
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

#[tokio::test(start_paused = true)]
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

#[tokio::test(start_paused = true)]
async fn rust_analyzer_server_status_feeds_progress_readiness() {
    let (client_w, server_r) = duplex(64 * 1024);
    let (server_w, client_r) = duplex(64 * 1024);
    let progress = ProgressTracker::new();
    let _connection =
        JsonRpcConnection::new(client_r, client_w, context(), Arc::clone(&progress), None);
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

#[tokio::test(start_paused = true)]
async fn connection_is_alive_until_the_server_stream_closes() {
    let (connection, server) = connect();
    assert!(connection.is_alive());
    drop(server);
    wait_until_dead(&connection).await;
}

#[tokio::test(start_paused = true)]
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

#[tokio::test(start_paused = true)]
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

#[tokio::test(start_paused = true)]
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

#[tokio::test(start_paused = true)]
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
