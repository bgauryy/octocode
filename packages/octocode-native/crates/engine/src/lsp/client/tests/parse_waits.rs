//! Waiting for many files' parses: an idle server costs one wait, not one per file.

use super::document_sync::{client_with_fake_server_using, send_server_frame};
use super::*;
use serde_json::json;
use tokio::time::Instant;

const WAIT_MS: u32 = 1_000;

fn publish(path: &str) -> serde_json::Value {
    json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics",
        "params":{"uri": crate::lsp::uri::path_to_uri(path).expect("uri"), "diagnostics":[]}})
}

fn progress(kind: &str) -> serde_json::Value {
    json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"load","value":{"kind":kind}}})
}

fn files(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("/ws/f{index}.c")).collect()
}

#[tokio::test(start_paused = true)]
async fn a_silent_idle_server_costs_one_wait() {
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let began = Instant::now();
    let parsed = client.wait_for_parses(files(8), WAIT_MS).await;
    assert!(parsed.is_empty(), "{parsed:?}");
    assert_eq!(began.elapsed(), Duration::from_millis(u64::from(WAIT_MS)));
}

/// A report for another document during a wait shows the server is parsing: the
/// file that missed its wait is unproven, and the next file still gets its turn.
#[tokio::test(start_paused = true)]
async fn a_report_for_another_file_keeps_the_waits_going() {
    let (client, _reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let all = files(3);
    let second = all[1].clone();
    let third = all[2].clone();
    let server = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        send_server_frame(&mut writer, publish(&second)).await;
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        send_server_frame(&mut writer, publish(&third)).await;
        writer
    });
    let parsed = client.wait_for_parses(all.clone(), WAIT_MS).await;
    assert_eq!(
        parsed,
        std::collections::HashSet::from([all[1].clone(), all[2].clone()]),
        "the first missed its wait; the other two were reported"
    );
    drop(server.await);
}

/// An open progress token is progress too: a busy server, publishing nothing
/// yet, keeps a full wait for every file.
#[tokio::test(start_paused = true)]
async fn a_busy_silent_server_keeps_a_wait_per_file() {
    let (client, _reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    send_server_frame(&mut writer, progress("begin")).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let began = Instant::now();
    let parsed = client.wait_for_parses(files(3), WAIT_MS).await;
    assert!(parsed.is_empty(), "{parsed:?}");
    assert_eq!(
        began.elapsed(),
        Duration::from_millis(3 * u64::from(WAIT_MS))
    );
    drop(writer);
}

/// Files already reported cost no wait, and a server that answers each is waited
/// for no longer than it takes.
#[tokio::test(start_paused = true)]
async fn reported_files_cost_no_wait() {
    let (client, _reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let all = files(4);
    for file in &all {
        send_server_frame(&mut writer, publish(file)).await;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
    let began = Instant::now();
    let parsed = client.wait_for_parses(all.clone(), WAIT_MS).await;
    assert_eq!(parsed.len(), all.len(), "{parsed:?}");
    assert_eq!(began.elapsed(), Duration::ZERO);
    drop(writer);
}
