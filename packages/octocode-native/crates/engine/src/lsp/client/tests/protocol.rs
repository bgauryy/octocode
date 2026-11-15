//! Wire protocol: `initialize` parameters, position encoding, and retryable errors.

use super::*;
use crate::error::Error;
use crate::lsp::client::lifecycle::{extract_position_encoding, initialize_params};
use crate::lsp::client::requests::is_retryable_error;
use serde_json::json;

fn rpc_error(error: Value) -> Error {
    Error::rpc(crate::error::RpcError::from_value(error))
}

#[test]
fn content_modified_detected_by_typed_error_code() {
    assert!(is_retryable_error(&rpc_error(
        json!({"code": -32801, "message": "content modified"})
    )));
}

#[test]
fn content_modified_not_triggered_by_phrase_in_payload() {
    // A payload that merely mentions the phrase or the number, with a
    // different code, must NOT be treated as ContentModified.
    assert!(!is_retryable_error(&rpc_error(json!({
        "code": -32603,
        "message": "docs say: content modified by user (-32801)"
    }))));
}

#[test]
fn content_modified_not_triggered_by_other_codes_or_untyped_text() {
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -328011, "message": "x"})
    )));
    // Rendered text is never parsed back into a code.
    assert!(!is_retryable_error(&Error::new(
        "LSP error: {\"code\":-32801,\"message\":\"content modified\"}",
    )));
}

#[test]
fn server_cancelled_is_retried_only_when_the_server_asks_for_a_retrigger() {
    assert!(is_retryable_error(&rpc_error(json!({
        "code": -32802, "message": "cancelled", "data": {"retriggerRequest": true}
    }))));
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -32802, "message": "cancelled"})
    )));
    assert!(!is_retryable_error(&rpc_error(json!({
        "code": -32802, "message": "cancelled", "data": {"retriggerRequest": false}
    }))));
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -32800, "message": "request cancelled"})
    )));
}

#[test]
fn extract_position_encoding_reads_server_choice() {
    // Server echoes the negotiated encoding.
    let result = json!({ "capabilities": { "positionEncoding": "utf-16" } });
    assert_eq!(
        extract_position_encoding(&result).as_deref(),
        Some("utf-16")
    );

    // A non-conformant server that ignored our utf-16-only advertisement.
    let result = json!({ "capabilities": { "positionEncoding": "utf-8" } });
    assert_eq!(extract_position_encoding(&result).as_deref(), Some("utf-8"));

    // Omitted ⇒ None (spec default is utf-16).
    let result = json!({ "capabilities": {} });
    assert_eq!(extract_position_encoding(&result), None);

    // No capabilities at all.
    assert_eq!(extract_position_encoding(&json!({})), None);
}

#[test]
fn initialize_advertises_push_diagnostics_so_servers_publish_them() {
    let config = LspLanguageServerConfig {
        command: "typescript-language-server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: std::env::temp_dir().to_string_lossy().into_owned(),
        language_id: Some("typescript".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let params = initialize_params(&config).expect("initialize params");
    assert_eq!(
        params.pointer("/capabilities/textDocument/publishDiagnostics/versionSupport"),
        Some(&json!(true)),
        "{params}"
    );
}

#[test]
fn initialize_declares_only_what_the_client_handles() {
    let params = initialize_params(&ts_config()).expect("initialize params");
    let stale = params
        .pointer("/capabilities/general/staleRequestSupport")
        .expect("staleRequestSupport");
    assert_eq!(stale["cancel"], json!(true));
    let retried: Vec<&str> = stale["retryOnContentModified"]
        .as_array()
        .expect("method list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for method in [
        "textDocument/definition",
        "textDocument/references",
        "textDocument/hover",
        "callHierarchy/incomingCalls",
        "workspace/symbol",
        "textDocument/diagnostic",
    ] {
        assert!(
            retried.contains(&method),
            "{method} is retried: {retried:?}"
        );
    }
    // We never send didSave, so we must not claim it.
    assert_eq!(
        params.pointer("/capabilities/textDocument/synchronization/didSave"),
        None
    );
    // serverStatus is rust-analyzer's extension only.
    assert_eq!(params.pointer("/capabilities/experimental"), None);
    assert_eq!(
        params.pointer("/capabilities/general/positionEncodings"),
        Some(&json!(["utf-16"]))
    );

    let mut rust = ts_config();
    rust.command = "/home/u/.cargo/bin/rust-analyzer".into();
    let params = initialize_params(&rust).expect("initialize params");
    assert_eq!(
        params.pointer("/capabilities/experimental/serverStatusNotification"),
        Some(&json!(true))
    );
}

/// A server picks kinds from the set the client advertises, and falls back to
/// the first LSP 1.0 kinds when none is sent: a struct then reads as a class
/// and an enum member as an enum. Every kind the protocol defines is advertised
/// for both symbol requests.
#[test]
fn symbol_kinds_are_advertised_for_document_and_workspace_symbols() {
    let params = initialize_params(&ts_config()).expect("initialize params");
    for pointer in [
        "/capabilities/textDocument/documentSymbol/symbolKind/valueSet",
        "/capabilities/workspace/symbol/symbolKind/valueSet",
    ] {
        let kinds = params
            .pointer(pointer)
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{pointer} is sent"));
        // Struct (23) and EnumMember (22) are newer than the LSP 1.0 set.
        for kind in [22, 23] {
            assert!(
                kinds.contains(&json!(kind)),
                "{pointer}: {kind} in {kinds:?}"
            );
        }
    }
}

#[test]
fn rust_analyzer_client_starts_headless_unless_the_caller_overrides() {
    let mut config = ts_config();
    config.command = "rust-analyzer".into();
    let client = NativeLspClient::new(config.clone());
    let options = client
        .inner
        .config
        .initialization_options
        .clone()
        .expect("headless defaults");
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(false))
    );
    assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
    assert_eq!(options.pointer("/cachePriming"), None);

    config.initialization_options = Some(json!({"cargo": {"buildScripts": {"enable": true}}}));
    let client = NativeLspClient::new(config);
    let options = client
        .inner
        .config
        .initialization_options
        .clone()
        .expect("options");
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(true))
    );
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));

    // typescript-language-server runs headless too (see
    // `typescript_headless_options`); servers with no defaults are untouched.
    let ts = NativeLspClient::new(ts_config());
    assert_eq!(
        ts.inner
            .config
            .initialization_options
            .as_ref()
            .and_then(|options| options.get("disableAutomaticTypingAcquisition")),
        Some(&json!(true))
    );
    let mut other = ts_config();
    other.command = "gopls".into();
    assert!(
        NativeLspClient::new(other)
            .inner
            .config
            .initialization_options
            .is_none()
    );
}

#[test]
fn initialize_offers_file_events_to_servers_that_register_for_them() {
    let params = initialize_params(&ts_config()).expect("initialize params");
    let watched = params
        .pointer("/capabilities/workspace/didChangeWatchedFiles")
        .expect("didChangeWatchedFiles");
    assert_eq!(watched["dynamicRegistration"], json!(true));
    assert_eq!(watched["relativePatternSupport"], json!(true));
}

#[tokio::test(start_paused = true)]
async fn a_server_that_registers_for_file_events_receives_them() {
    use super::document_sync::{
        client_with_fake_server_using, read_client_frame, send_server_frame,
    };
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    assert!(!client.watches_files().await);
    send_server_frame(
        &mut writer,
        json!({"jsonrpc":"2.0","id":"w","method":"client/registerCapability","params":{"registrations":[{
            "id":"watch","method":"workspace/didChangeWatchedFiles",
            "registerOptions":{"watchers":[{"globPattern":"**/*.ts"}]}
        }]}}),
    )
    .await;
    let reply = read_client_frame(&mut reader).await;
    assert_eq!(reply["id"], "w");
    assert!(client.watches_files().await, "the registration is noted");
    let root = std::env::temp_dir();
    let unchanged = |_: &str, before: &str, after: &str| before == after;
    client
        .sync_watched_files(|previous| {
            std::future::ready(WatchedFileUpdate::new(
                &root,
                previous.as_ref(),
                std::collections::BTreeMap::new(),
                unchanged,
            ))
        })
        .await
        .expect("initial snapshot");
    client
        .sync_watched_files(|previous| {
            std::future::ready(WatchedFileUpdate::new(
                &root,
                previous.as_ref(),
                std::collections::BTreeMap::from([("new.ts".to_owned(), "new".to_owned())]),
                unchanged,
            ))
        })
        .await
        .expect("notify");
    let event = read_client_frame(&mut reader).await;
    assert_eq!(event["method"], "workspace/didChangeWatchedFiles");
    assert_eq!(
        event["params"]["changes"][0]["uri"],
        crate::lsp::uri::path_to_uri(&root.join("new.ts").to_string_lossy()).expect("uri")
    );
}

#[tokio::test(start_paused = true)]
async fn watched_snapshots_are_captured_in_commit_order() {
    use super::document_sync::{client_with_fake_server_using, read_client_frame};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let (client, mut reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let root = std::env::temp_dir();
    let stage = AtomicUsize::new(0);
    let (release, released) = tokio::sync::oneshot::channel();
    let (first_root, first_stage) = (&root, &stage);
    let mut first = Box::pin(client.sync_watched_files(|previous| async move {
        assert!(previous.is_none());
        first_stage.store(1, Ordering::SeqCst);
        released.await.expect("release first capture");
        first_stage.store(2, Ordering::SeqCst);
        WatchedFileUpdate::new(
            first_root,
            previous.as_ref(),
            BTreeMap::from([("a.rs".to_owned(), "one".to_owned())]),
            |_, before, after| before == after,
        )
    }));
    tokio::select! {
        biased;
        result = &mut first => panic!("first capture must wait: {result:?}"),
        () = std::future::ready(()) => {}
    }
    assert_eq!(stage.load(Ordering::SeqCst), 1);
    let mut second = Box::pin(client.sync_watched_files(|previous| {
        assert_eq!(
            stage.load(Ordering::SeqCst),
            2,
            "capture is inside the lock"
        );
        assert_eq!(
            previous.as_ref().and_then(|files| files.get("a.rs")),
            Some(&"one".to_owned()),
            "the second capture compares with the committed first snapshot"
        );
        stage.store(3, Ordering::SeqCst);
        std::future::ready(WatchedFileUpdate::new(
            &root,
            previous.as_ref(),
            BTreeMap::from([
                ("a.rs".to_owned(), "one".to_owned()),
                ("b.rs".to_owned(), "two".to_owned()),
            ]),
            |_, before, after| before == after,
        ))
    }));
    tokio::select! {
        biased;
        result = &mut second => panic!("second capture must wait: {result:?}"),
        () = std::future::ready(()) => {}
    }
    assert_eq!(stage.load(Ordering::SeqCst), 1);
    release.send(()).expect("release");
    first.await.expect("first sync");
    second.await.expect("second sync");
    let event = read_client_frame(&mut reader).await;
    assert_eq!(
        event["params"]["changes"].as_array().expect("events").len(),
        1
    );
    assert_eq!(event["params"]["changes"][0]["type"], 1);
    assert!(
        event["params"]["changes"][0]["uri"]
            .as_str()
            .expect("uri")
            .ends_with("/b.rs")
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_watched_event_write_preserves_the_committed_snapshot() {
    use super::document_sync::client_with_fake_server_using;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    let (client, reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let root = std::env::temp_dir();
    let first = BTreeMap::from([("a.rs".to_owned(), "before".to_owned())]);
    client
        .sync_watched_files(|previous| {
            std::future::ready(WatchedFileUpdate::new(
                &root,
                previous.as_ref(),
                first.clone(),
                |_, before, after| before == after,
            ))
        })
        .await
        .expect("baseline");
    drop(reader);
    let prepared = AtomicBool::new(false);
    let result = client
        .sync_watched_files(|previous| {
            prepared.store(true, Ordering::SeqCst);
            std::future::ready(WatchedFileUpdate::new(
                &root,
                previous.as_ref(),
                BTreeMap::from([("a.rs".to_owned(), "after".to_owned())]),
                |_, before, after| before == after,
            ))
        })
        .await;
    assert!(
        prepared.load(Ordering::SeqCst),
        "the write, not capture, failed"
    );
    assert!(result.is_err());
    assert_eq!(
        client
            .inner
            .watched_files
            .lock()
            .expect("snapshot")
            .as_ref(),
        Some(&first)
    );
}

#[tokio::test]
async fn a_failed_first_watched_capture_requires_a_fresh_session() {
    use super::document_sync::client_with_fake_server_using;
    use std::collections::BTreeMap;

    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    assert!(client.is_alive().await);
    let failed = client
        .sync_watched_files(|previous| async move {
            assert!(previous.is_none());
            Err::<WatchedFileUpdate, _>(Error::new("capture failed"))
        })
        .await
        .expect_err("initial capture fails");
    assert_eq!(failed.reason, "capture failed");

    // The server's original file disappeared before capture became readable.
    // No successful snapshot exists from which to derive its Deleted event.
    let root = std::env::temp_dir();
    let retry = client
        .sync_watched_files(|previous| {
            std::future::ready(WatchedFileUpdate::new(
                &root,
                previous.as_ref(),
                BTreeMap::new(),
                |_, before, after| before == after,
            ))
        })
        .await;
    assert!(
        retry.is_err(),
        "a retry cannot silently accept an event-free baseline"
    );
    assert!(
        !client.is_alive().await,
        "the pool must replace the stale session"
    );
    assert!(!client.has_watched_snapshot());

    let (fresh, _fresh_reader, _fresh_writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    fresh
        .sync_watched_files(|previous| {
            assert!(previous.is_none());
            std::future::ready(WatchedFileUpdate::new(
                &root,
                previous.as_ref(),
                BTreeMap::new(),
                |_, before, after| before == after,
            ))
        })
        .await
        .expect("a freshly indexed session establishes the current baseline");
    assert!(fresh.is_alive().await);
    assert!(fresh.has_watched_snapshot());
}

#[tokio::test]
async fn a_cancelled_first_watched_capture_requires_a_fresh_session() {
    use super::document_sync::client_with_fake_server_using;
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let mut capture =
        Box::pin(client.sync_watched_files(|_| {
            std::future::pending::<crate::error::Result<WatchedFileUpdate>>()
        }));
    tokio::select! {
        biased;
        result = &mut capture => panic!("capture must wait: {result:?}"),
        () = std::future::ready(()) => {}
    }
    assert!(client.is_alive().await);
    drop(capture);
    assert!(
        !client.is_alive().await,
        "dropped bootstrap capture cannot be reused"
    );
    assert!(!client.has_watched_snapshot());
}
