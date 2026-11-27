//! Waiting for many files' parses: an idle server costs one quiet window, a
//! working one is waited for until it reports them or the caller's deadline.

use super::document_sync::{client_with_fake_server_using, send_server_frame};
use super::*;
use serde_json::json;

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

#[test]
fn a_silent_idle_server_costs_one_wait() {
    paused(|clock| async move {
        let (client, _reader, _writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        let began = clock.now();
        let parsed = client.wait_for_parses(files(8), WAIT_MS).await;
        assert!(parsed.is_empty(), "{parsed:?}");
        assert_eq!(
            clock.elapsed(began),
            Duration::from_millis(u64::from(WAIT_MS))
        );
    });
}

/// A report for another document during a wait shows the server is parsing: the
/// file that missed its wait is unproven, and the next file still gets its turn.
#[test]
fn a_report_for_another_file_keeps_the_waits_going() {
    paused(|clock| async move {
        let (client, _reader, mut writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        let all = files(3);
        let second = all[1].clone();
        let third = all[2].clone();
        let task_clock = clock.clone();
        let server = tokio::spawn(async move {
            task_clock.sleep(Duration::from_millis(100)).await;
            send_server_frame(&mut writer, publish(&second)).await;
            task_clock.sleep(Duration::from_millis(1_500)).await;
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
    });
}

/// A token left open with nothing new said is no work: a server that only
/// holds one costs one quiet window, not a wait per file.
#[test]
fn an_open_token_that_says_nothing_new_costs_one_window() {
    paused(|clock| async move {
        let (client, _reader, mut writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        send_server_frame(&mut writer, progress("begin")).await;
        clock.sleep(Duration::from_millis(10)).await;
        let began = clock.now();
        let parsed = client.wait_for_parses(files(3), WAIT_MS).await;
        assert!(parsed.is_empty(), "{parsed:?}");
        assert_eq!(
            clock.elapsed(began),
            Duration::from_millis(u64::from(WAIT_MS))
        );
        drop(writer);
    });
}

/// A server that keeps reporting its work is waited for past any fixed
/// window, until it reports the files or the caller's deadline ends the
/// wait; then the files it reported count and the rest stay unproven.
#[test]
fn a_working_server_is_waited_for_until_it_reports_or_the_deadline() {
    paused(|clock| async move {
        let (client, _reader, mut writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        let all = files(3);
        let late = all[2].clone();
        let reported = all[..2].to_vec();
        let task_clock = clock.clone();
        let server = tokio::spawn(async move {
            send_server_frame(&mut writer, progress("begin")).await;
            for step in 0..40 {
                task_clock
                    .sleep(Duration::from_millis(u64::from(WAIT_MS) / 2))
                    .await;
                send_server_frame(&mut writer, progress("report")).await;
                if step == 12 {
                    for file in &reported {
                        send_server_frame(&mut writer, publish(file)).await;
                    }
                }
            }
            writer
        });
        let deadline = clock.now() + Duration::from_millis(10 * u64::from(WAIT_MS));
        let began = clock.now();
        let parsed = crate::lsp::with_caller_deadline(
            Some(deadline),
            client.wait_for_parses(all.clone(), WAIT_MS),
        )
        .await;
        let waited = clock.elapsed(began);
        assert_eq!(
            parsed,
            std::collections::HashSet::from([all[0].clone(), all[1].clone()]),
            "the two reported while the server worked count; {late} never was"
        );
        assert!(
            waited > Duration::from_millis(6 * u64::from(WAIT_MS)) && clock.now() <= deadline,
            "waited {waited:?}: past the reports, within the deadline"
        );
        drop(server.await);
    });
}

/// The first file the server never reports ends nothing early: a file it
/// reported before the wait counts however the wait ended.
#[test]
fn a_file_reported_before_an_unreported_one_still_counts() {
    paused(|clock| async move {
        let (client, _reader, mut writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        let all = files(3);
        send_server_frame(&mut writer, publish(&all[2])).await;
        clock.sleep(Duration::from_millis(10)).await;
        let parsed = client.wait_for_parses(all.clone(), WAIT_MS).await;
        assert_eq!(parsed, std::collections::HashSet::from([all[2].clone()]));
        drop(writer);
    });
}

/// Files already reported cost no wait, and a server that answers each is waited
/// for no longer than it takes.
#[test]
fn reported_files_cost_no_wait() {
    paused(|clock| async move {
        let (client, _reader, mut writer) =
            client_with_fake_server_using(client_on(&clock, ts_config()), 64 * 1024).await;
        let all = files(4);
        for file in &all {
            send_server_frame(&mut writer, publish(file)).await;
        }
        clock.sleep(Duration::from_millis(10)).await;
        let began = clock.now();
        let parsed = client.wait_for_parses(all.clone(), WAIT_MS).await;
        assert_eq!(parsed.len(), all.len(), "{parsed:?}");
        assert_eq!(clock.elapsed(began), Duration::ZERO);
        drop(writer);
    });
}
