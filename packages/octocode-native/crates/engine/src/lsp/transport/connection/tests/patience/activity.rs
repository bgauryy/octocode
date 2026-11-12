//! Any message a server sends is a sign of work; a silent one is named idle.
use super::*;

/// A server that does its work through another process (Metals over its
/// build server) shows little CPU of its own but keeps talking: any message
/// it sends is a sign of work, so the request waits for it. Once it falls
/// silent the wait ends, and the error says the server was idle and for how
/// long, not that it was starting.
#[tokio::test(start_paused = true)]
async fn a_talking_server_is_waited_for_and_an_idle_one_is_named_idle() {
    let (connection, mut server) = connect_watched(
        Some(cpu_clock(|_| Duration::from_secs(5))),
        ProgressTracker::new(),
    );
    let talking = tokio::spawn(async move {
        let request = server.read_message().await;
        for _ in 0..9 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            server
                .send(json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":4,"message":"compiling"}}))
                .await;
        }
        server
            .send(json!({"jsonrpc":"2.0","id":request["id"],"result":"late"}))
            .await;
        server
    });
    let result = connection
        .request("workspace/symbol", Value::Null, BASE_MS)
        .await
        .expect("a talking server is waited for");
    assert_eq!(result, json!("late"));
    let _server = talking.await.expect("server");
    let started = Instant::now();
    let error = connection
        .request("workspace/symbol", Value::Null, BASE_MS)
        .await
        .expect_err("a silent server is cut");
    assert_eq!(started.elapsed(), Duration::from_millis(1_000));
    assert!(
        error
            .reason
            .contains("workspace/symbol timed out after 1000ms")
            && error.reason.contains("showed no activity")
            && error.reason.contains("in its last 250ms"),
        "{}",
        error.reason
    );
}
