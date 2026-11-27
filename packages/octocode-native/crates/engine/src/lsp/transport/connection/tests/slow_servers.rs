//! A slow server is not a wedged one: what retires a connection, what a
//! request queued behind other frames waits for, and how a request names a
//! failure it did not cause.

use super::*;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

/// A server that keeps logging while it works and answers only the ids the
/// test hands it. Its log lines are activity, so a request waits on it past
/// the base timeout until the caller's deadline ends the wait.
fn talking_server(
    mut server: Server,
    clock: &TestClock,
) -> (
    mpsc::UnboundedSender<Value>,
    tokio::task::JoinHandle<Vec<Value>>,
) {
    let (answer, mut answers) = mpsc::unbounded_channel::<Value>();
    let clock = clock.clone();
    let task = tokio::spawn(async move {
        let mut received = Vec::new();
        loop {
            tokio::select! {
                biased;
                id = answers.recv() => match id {
                    Some(id) => {
                        server
                            .send(json!({"jsonrpc":"2.0","id":id,"result":"answered"}))
                            .await;
                    }
                    None => return received,
                },
                message = server.read_message() => received.push(message),
                () = clock.sleep(Duration::from_millis(100)) => {
                    server
                        .send(json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":4,"message":"still loading"}}))
                        .await;
                }
            }
        }
    });
    (answer, task)
}

/// Two requests whose caller's deadline ends their waits while the server
/// keeps working time out without retiring the server: it is slow, not
/// wedged, and the next request is answered on the same connection.
#[test]
fn waits_the_callers_deadline_ends_on_a_working_server_keep_the_connection() {
    paused(|clock| async move {
        let (connection, server) = connect(&clock);
        let connection = Arc::new(connection);
        let (answer, server) = talking_server(server, &clock);
        for _ in 0..RETIRE_AFTER_TIMEOUTS {
            let deadline = clock.now() + Duration::from_secs(3);
            let error = with_caller_deadline(
                Some(deadline),
                connection.request("textDocument/references", Value::Null, 1_000),
            )
            .await
            .expect_err("the caller's deadline ends the wait");
            assert_eq!(error.kind(), &ErrorKind::Timeout);
            assert!(error.reason.contains("still working"), "{}", error.reason);
            assert!(
                connection.is_alive(),
                "a server that worked until the deadline is kept: {}",
                error.reason
            );
        }
        let third = tokio::spawn({
            let connection = Arc::clone(&connection);
            async move {
                connection
                    .request("textDocument/definition", Value::Null, 1_000)
                    .await
            }
        });
        while pending_len(&connection) != Some(1) {
            tokio::task::yield_now().await;
        }
        let id = u64::from(RETIRE_AFTER_TIMEOUTS) + 1;
        answer.send(json!(id)).expect("server alive");
        assert_eq!(
            third.await.expect("join").expect("the same server answers"),
            json!("answered")
        );
        assert!(connection.is_alive());
        drop(answer);
        let received = server.await.expect("server");
        let cancels = received
            .iter()
            .filter(|message| message["method"] == "$/cancelRequest")
            .count();
        assert_eq!(
            cancels,
            usize::try_from(RETIRE_AFTER_TIMEOUTS).expect("count"),
            "each abandoned request is still cancelled: {received:?}"
        );
    });
}

/// An operation refused by, or pending on, a connection that another
/// request's timeouts retired names itself first, says the failure came
/// earlier when it did, and keeps that cause.
#[test]
fn a_request_failed_by_the_connection_names_itself_and_the_cause() {
    paused(|clock| async move {
        let (connection, _server) = connect(&clock);
        let connection = Arc::new(connection);
        let pending = tokio::spawn({
            let connection = Arc::clone(&connection);
            async move {
                connection
                    .request("textDocument/references", Value::Null, 60_000)
                    .await
            }
        });
        while pending_len(&connection) != Some(1) {
            tokio::task::yield_now().await;
        }
        for _ in 0..RETIRE_AFTER_TIMEOUTS {
            let error = connection
                .request("textDocument/prepareCallHierarchy", Value::Null, 50)
                .await
                .expect_err("the server answers nothing");
            assert!(
                error
                    .reason
                    .starts_with("LSP request textDocument/prepareCallHierarchy"),
                "{}",
                error.reason
            );
        }
        assert!(!connection.is_alive());
        let drained = pending
            .await
            .expect("join")
            .expect_err("the pending request fails with the connection");
        let refused = connection
            .request("textDocument/references", Value::Null, 1_000)
            .await
            .expect_err("a retired connection takes no request");
        let unsent = connection
            .notify("textDocument/didOpen", json!({}))
            .await
            .expect_err("a retired connection takes no notification");
        let unchecked = connection
            .ensure_alive()
            .expect_err("a retired connection is not alive");
        for (error, names) in [
            (
                drained,
                "LSP request textDocument/references was not answered: ",
            ),
            (
                refused,
                "LSP request textDocument/references was not sent: the LSP connection failed earlier: ",
            ),
            (
                unsent,
                "LSP notification textDocument/didOpen was not sent: the LSP connection failed earlier: ",
            ),
            (unchecked, "the LSP connection failed earlier: "),
        ] {
            assert_eq!(error.kind(), &ErrorKind::ConnectionClosed);
            assert!(error.reason.starts_with(names), "{}", error.reason);
            assert!(
                error.reason.contains("textDocument/prepareCallHierarchy")
                    && error.reason.contains("retired"),
                "the cause is kept: {}",
                error.reason
            );
        }
    });
}

/// Drains the client's output `chunk` bytes at a time, one read per `pace`
/// of the test clock, until it has read `total` bytes or `stop_after` reads.
fn slow_reader(
    mut server: Server,
    clock: &TestClock,
    chunk: usize,
    pace: Duration,
    stop_after: Option<usize>,
) -> tokio::task::JoinHandle<(Server, usize)> {
    let clock = clock.clone();
    tokio::spawn(async move {
        let mut buffer = vec![0_u8; chunk];
        let mut read = 0;
        let mut reads = 0;
        loop {
            if stop_after.is_some_and(|limit| reads >= limit) {
                // Holds the pipe open without reading: a server that stopped
                // reading its input.
                std::future::pending::<()>().await;
            }
            clock.sleep(pace).await;
            match server.reader.read(&mut buffer).await {
                Ok(0) | Err(_) => return (server, read),
                Ok(taken) => {
                    read += taken;
                    reads += 1;
                }
            }
        }
    })
}

/// A large `didOpen` to a server that reads slowly but steadily is written
/// whole however long it takes, and a small notification queued behind it
/// waits its turn: neither counts its time in the queue, and the connection
/// stays alive.
#[test]
fn notifications_behind_a_slowly_read_frame_wait_their_turn() {
    paused(|clock| async move {
        let (connection, server) = connect_with_buffer(16 * 1024, &clock);
        let connection = Arc::new(connection);
        let reader = slow_reader(server, &clock, 16 * 1024, Duration::from_millis(200), None);
        let text = "x".repeat(3 * 1024 * 1024);
        let started = clock.now();
        let large = tokio::spawn({
            let connection = Arc::clone(&connection);
            async move {
                connection
                    .notify("textDocument/didOpen", json!({ "text": text }))
                    .await
            }
        });
        tokio::task::yield_now().await;
        let small = tokio::spawn({
            let connection = Arc::clone(&connection);
            async move { connection.notify("initialized", json!({})).await }
        });
        large
            .await
            .expect("join")
            .expect("a steadily read frame is written whole");
        small
            .await
            .expect("join")
            .expect("a frame's place in the queue is not a fault");
        assert!(
            clock.elapsed(started) > Duration::from_secs(10),
            "the reader paced the writes: {:?}",
            clock.elapsed(started)
        );
        assert!(connection.is_alive());
        drop(reader);
    });
}

/// A server that stops reading partway through a large frame still fails
/// the write, and with it the connection.
#[test]
fn a_reader_that_stops_draining_still_fails_the_write() {
    paused(|clock| async move {
        let (connection, server) = connect_with_buffer(16 * 1024, &clock);
        let reader = slow_reader(
            server,
            &clock,
            16 * 1024,
            Duration::from_millis(200),
            Some(4),
        );
        let text = "x".repeat(3 * 1024 * 1024);
        let error = connection
            .notify("textDocument/didOpen", json!({ "text": text }))
            .await
            .expect_err("the server stopped reading");
        assert_eq!(error.kind(), &ErrorKind::Timeout);
        assert!(!connection.is_alive());
        reader.abort();
    });
}
