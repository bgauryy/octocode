use super::*;

#[test]
fn notify_stall_scales_with_payload_size() {
    assert_eq!(notify_stall_ms(0), NOTIFY_BASE_DEADLINE_MS);
    assert_eq!(notify_stall_ms(1_024), NOTIFY_BASE_DEADLINE_MS);
    assert_eq!(
        notify_stall_ms(3 * 1024 * 1024),
        NOTIFY_BASE_DEADLINE_MS + 3 * NOTIFY_MS_PER_MIB
    );
    assert!(notify_stall_ms(8 * 1024 * 1024) > notify_stall_ms(1024 * 1024));
}

#[test]
fn request_with_partials_collects_progress_before_final_response() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
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
    });
}

#[test]
fn partial_chunks_with_a_kind_field_are_results_not_progress() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
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
    });
}

#[test]
fn dropped_partial_request_releases_its_token_buffer() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
        let dropped = clock
            .timeout(
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
    });
}

#[test]
fn rust_analyzer_server_status_feeds_progress_readiness() {
    paused(|clock| async move {
        let (client_w, server_r) = duplex(64 * 1024);
        let (server_w, client_r) = duplex(64 * 1024);
        let progress = ProgressTracker::new(clock.lsp());
        let _connection = JsonRpcConnection::new(
            client_r,
            client_w,
            context(),
            Arc::clone(&progress),
            None,
            clock.lsp(),
        );
        let mut server = Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        };
        let mut active = progress.subscribe();
        server
        .send(json!({"jsonrpc":"2.0","method":"experimental/serverStatus","params":{"health":"ok","quiescent":false}}))
        .await;
        clock
            .timeout(Duration::from_secs(5), active.wait_for(|count| *count == 1))
            .await
            .expect("non-quiescent status opens a progress token")
            .expect("tracker alive");
        server
        .send(json!({"jsonrpc":"2.0","method":"experimental/serverStatus","params":{"health":"ok","quiescent":true}}))
        .await;
        clock
            .timeout(Duration::from_secs(5), active.wait_for(|count| *count == 0))
            .await
            .expect("quiescent status closes it")
            .expect("tracker alive");
    });
}

#[test]
fn rust_analyzer_server_status_keeps_its_last_health() {
    paused(|clock| async move {
        let (client_w, server_r) = duplex(64 * 1024);
        let (server_w, client_r) = duplex(64 * 1024);
        let progress = ProgressTracker::new(clock.lsp());
        let _connection = JsonRpcConnection::new(
            client_r,
            client_w,
            context(),
            Arc::clone(&progress),
            None,
            clock.lsp(),
        );
        let mut server = Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        };
        let mut active = progress.subscribe();
        // The non-quiescent status that follows each one shows it was read.
        server
        .send(json!({"jsonrpc":"2.0","method":"experimental/serverStatus","params":{"health":"error","message":"Failed to load workspaces.","quiescent":false}}))
        .await;
        clock
            .timeout(Duration::from_secs(5), active.wait_for(|count| *count == 1))
            .await
            .expect("status read")
            .expect("tracker alive");
        let reported = progress.health().expect("the failure is kept");
        assert_eq!(reported.level, crate::lsp::transport::HealthLevel::Error);
        assert_eq!(
            reported.message.as_deref(),
            Some("Failed to load workspaces.")
        );
        server
        .send(json!({"jsonrpc":"2.0","method":"experimental/serverStatus","params":{"health":"ok","quiescent":true}}))
        .await;
        clock
            .timeout(Duration::from_secs(5), active.wait_for(|count| *count == 0))
            .await
            .expect("status read")
            .expect("tracker alive");
        assert_eq!(progress.health(), None, "ok clears the failure");
    });
}

/// Declining Metals' build import leaves it analysing open files only: the server
/// is then not fully working, and says nothing more itself.
#[test]
fn a_declined_build_import_is_kept_as_the_servers_health() {
    paused(|clock| async move {
        let (client_w, server_r) = duplex(64 * 1024);
        let (server_w, client_r) = duplex(64 * 1024);
        let progress = ProgressTracker::new(clock.lsp());
        let _connection = JsonRpcConnection::new(
            client_r,
            client_w,
            context(),
            Arc::clone(&progress),
            None,
            clock.lsp(),
        );
        let mut server = Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        };
        let prompt = |id: u64, title: &str| {
            json!({"jsonrpc":"2.0","id":id,"method":"window/showMessageRequest","params":{
            "type":3,"message":"New sbt workspace detected, would you like to import the build?",
            "actions":[{"title":title},{"title":"Not now"}]}})
        };
        server.send(prompt(1, "Open logs")).await;
        assert_eq!(server.read_message().await["result"], Value::Null);
        assert_eq!(progress.health(), None, "a prompt that imports nothing");
        server.send(prompt(2, "Import build")).await;
        assert_eq!(
            server.read_message().await["result"],
            Value::Null,
            "still declined"
        );
        let reported = progress.health().expect("the declined import is kept");
        assert_eq!(reported.level, crate::lsp::transport::HealthLevel::Warning);
        assert!(
            reported
                .message
                .as_deref()
                .is_some_and(|message| message.contains("import the build")),
            "{reported:?}"
        );
    });
}

#[test]
fn connection_is_alive_until_the_server_stream_closes() {
    paused(|clock| async move {
        let (connection, server) = connect(&clock);
        assert!(connection.is_alive());
        drop(server);
        wait_until_dead(&connection, &clock).await;
    });
}

#[test]
fn connection_caches_push_diagnostics_for_bounded_on_demand_reads() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
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
        let document = [("file:///workspace/a.ts".to_owned(), Some(3))];
        let report = connection
            .wait_for_push_diagnostics(&document, 1_000)
            .await
            .pop()
            .flatten()
            .expect("push diagnostics report");
        assert_eq!(report["kind"], "full");
        assert_eq!(report["version"], 3);
        assert_eq!(report["items"][0]["message"], "broken");

        connection.clear_push_diagnostics("file:///workspace/a.ts");
        assert!(
            connection
                .wait_for_push_diagnostics(&document, 1)
                .await
                .pop()
                .flatten()
                .is_none()
        );
    });
}

#[test]
fn lengthless_frame_is_skipped_and_string_ids_route() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
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
    });
}

#[test]
fn concurrent_requests_resolve_out_of_order() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
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
    });
}

#[test]
fn rpc_errors_are_typed_through_the_connection() {
    paused(|clock| async move {
        let (connection, mut server) = connect(&clock);
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
    });
}

/// jdtls reports a project it could not import (its Gradle cannot run on this
/// JDK) as a `language/eventNotification` naming the project: the server is
/// then not answering for it, which is its health until a project status of
/// `OK`; the `WARNING` status that follows the failure keeps it.
#[test]
fn a_failed_project_import_is_the_servers_health_until_the_project_is_ok() {
    paused(|clock| async move {
        let (client_w, server_r) = duplex(64 * 1024);
        let (server_w, client_r) = duplex(64 * 1024);
        let progress = ProgressTracker::new(clock.lsp());
        let _connection = JsonRpcConnection::new(
            client_r,
            client_w,
            context(),
            Arc::clone(&progress),
            None,
            clock.lsp(),
        );
        let mut server = Server {
            reader: BufReader::new(server_r),
            writer: server_w,
        };
        let mut active = progress.subscribe();
        let message = "Can't use Java 27.0.1.0.3 and Gradle 9.7.1 to import Gradle project server.";
        server
        .send(json!({"jsonrpc":"2.0","method":"language/eventNotification","params":{"eventType":300,"data":{"projectUri":"file:///repo/server/","message":message,"highestJavaVersion":"26"}}}))
        .await;
        server
        .send(json!({"jsonrpc":"2.0","method":"language/status","params":{"type":"ProjectStatus","message":"WARNING"}}))
        .await;
        // A progress wave after them shows both were read, in order.
        server
        .send(json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"fence","value":{"kind":"begin","title":"t"}}}))
        .await;
        clock
            .timeout(Duration::from_secs(5), active.wait_for(|count| *count == 1))
            .await
            .expect("notifications read")
            .expect("tracker alive");
        let reported = progress.health().expect("the failed import is kept");
        assert_eq!(reported.level, crate::lsp::transport::HealthLevel::Error);
        assert_eq!(reported.project.as_deref(), Some("/repo/server/"));
        let text = reported.message.unwrap_or_default();
        assert!(text.contains("/repo/server"), "{text}");
        assert!(text.contains(message), "{text}");
        server
        .send(json!({"jsonrpc":"2.0","method":"language/status","params":{"type":"ProjectStatus","message":"OK"}}))
        .await;
        server
        .send(json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"fence","value":{"kind":"end"}}}))
        .await;
        clock
            .timeout(Duration::from_secs(5), active.wait_for(|count| *count == 0))
            .await
            .expect("notifications read")
            .expect("tracker alive");
        assert_eq!(progress.health(), None, "an OK project status clears it");
    });
}
