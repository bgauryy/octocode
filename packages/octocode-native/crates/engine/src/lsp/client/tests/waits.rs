//! A client remembers what its callers waited on (server start, project
//! load), so a call can say what it waited for.

use super::document_sync::{client_with_fake_server_using, read_client_frame, send_server_frame};
use super::*;
use serde_json::json;
use tokio::time::Instant;

/// A project-load wait (the first open's settle and load) is
/// reported to a call that was running during it, as the sum of what it
/// overlapped; a call that began after it saw nothing wait.
#[test]
fn project_load_waits_are_reported_to_the_calls_that_overlapped_them() {
    paused(|clock| async move {
        let tmp = TmpDir::new();
        let source = tmp.0.join("a.ts");
        let content = "function foo() {}\nfoo();\n";
        std::fs::write(&source, content).expect("source");
        let path = source.to_string_lossy().into_owned();
        let (client, mut reader, mut writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        let before = clock.now();
        assert_eq!(client.waits_since(before), ServerWaits::default());
        client.wait_for_ready(Some(300)).await.expect("settles");
        let task_clock = clock.clone();
        let server = tokio::spawn(async move {
            assert_eq!(
                read_client_frame(&mut reader).await["method"],
                "textDocument/didOpen"
            );
            task_clock.sleep(Duration::from_millis(100)).await;
            let load = |kind: &str| json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"load","value":{"kind":kind}}});
            send_server_frame(&mut writer, load("begin")).await;
            task_clock.sleep(Duration::from_millis(400)).await;
            send_server_frame(&mut writer, load("end")).await;
            (reader, writer)
        });
        client
            .open_document_and_wait(path, content, Some(400), Some(5_000))
            .await
            .expect("open");
        let waits = client.waits_since(before);
        assert_eq!(waits.starting, None, "this client was not started here");
        assert_eq!(
            waits.indexing,
            Some(Duration::from_millis(700)),
            "the first open's settle and project load (a TypeScript server owes no startup settle)"
        );
        let after = clock.now();
        assert_eq!(client.waits_since(after), ServerWaits::default());
        drop(server.await);
    });
}

/// Concurrent calls wait on the same load, each logging its own interval; a
/// call reports the time the load took, not the sum of the overlapping logs.
#[test]
fn overlapping_waits_are_reported_once() {
    paused(|clock| async move {
        let client = client_on(&clock, ts_config());
        let before = clock.now();
        let first = clock.now();
        clock.sleep(Duration::from_millis(100)).await;
        let second = clock.now();
        clock.sleep(Duration::from_millis(300)).await;
        client.record_wait(super::super::waits::WaitKind::Indexing, first);
        clock.sleep(Duration::from_millis(100)).await;
        client.record_wait(super::super::waits::WaitKind::Indexing, second);
        assert_eq!(
            client.waits_since(before).indexing,
            Some(Duration::from_millis(500)),
            "the union of [0, 400] and [100, 500]"
        );
    });
}

/// Starting the server process is a wait of its own, kept apart from the
/// project load that follows.
#[test]
#[ignore = "opt-in process integration: Node server and wall-clock start"]
fn a_slow_server_start_is_reported_as_starting() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (_tmp, _root, config) = configurable_server(
            "lsp-waits",
            &[("FAKE_INIT_DELAY_MS", "250".to_owned())],
            None,
        );
        let client = NativeLspClient::new(config);
        let before = Instant::now();
        client.start().await.expect("fake server starts");
        let waits = client.waits_since(before);
        assert!(
            waits
                .starting
                .is_some_and(|starting| starting >= Duration::from_millis(250)),
            "{waits:?}"
        );
        assert_eq!(waits.indexing, None);
        client.stop().await.unwrap();
    });
}
