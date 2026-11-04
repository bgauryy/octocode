use super::document_sync::{client_with_fake_server_using, read_client_frame, send_server_frame};
use super::*;
use serde_json::json;
use tokio::time::Instant;

async fn symbols_in_generation(client: NativeLspClient, path: String) -> Arc<Value> {
    RESPONSE_SCOPE
        .scope(
            std::cell::RefCell::new(ResponseScope {
                generation: "same-project".into(),
                call: None,
                ..Default::default()
            }),
            client.get_document_symbols(path),
        )
        .await
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_late_old_version_response_cannot_replace_a_fresh_requests_answer() {
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let tmp = TmpDir::new();
    let path = tmp.0.join("cache.ts").to_string_lossy().into_owned();
    std::fs::write(&path, "old").unwrap();
    client.open_document(path.clone(), "old").await.unwrap();
    assert_eq!(
        read_client_frame(&mut reader).await["method"],
        "textDocument/didOpen"
    );
    let old = tokio::spawn(symbols_in_generation(client.clone(), path.clone()));
    let old_request = read_client_frame(&mut reader).await;
    std::fs::write(&path, "new").unwrap();
    client.open_document(path.clone(), "new").await.unwrap();
    assert_eq!(
        read_client_frame(&mut reader).await["method"],
        "textDocument/didChange"
    );
    let new = tokio::spawn(symbols_in_generation(client.clone(), path.clone()));
    let new_request = read_client_frame(&mut reader).await;
    send_server_frame(
        &mut writer,
        json!({"jsonrpc":"2.0","id":new_request["id"],"result":["new"]}),
    )
    .await;
    assert_eq!(*new.await.unwrap(), json!(["new"]));
    send_server_frame(
        &mut writer,
        json!({"jsonrpc":"2.0","id":old_request["id"],"result":["old"]}),
    )
    .await;
    assert_eq!(*old.await.unwrap(), json!(["old"]));
    let fresh = tokio::spawn(symbols_in_generation(client, path));
    let fresh_request = read_client_frame(&mut reader).await;
    send_server_frame(
        &mut writer,
        json!({"jsonrpc":"2.0","id":fresh_request["id"],"result":["fresh"]}),
    )
    .await;
    assert_eq!(*fresh.await.unwrap(), json!(["fresh"]));
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_request_does_not_block_or_poison_a_concurrent_request() {
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let slow = tokio::spawn({
        let client = client.clone();
        async move { client.workspace_symbol("slow".into()).await }
    });
    let slow_request = read_client_frame(&mut reader).await;
    let fast = tokio::spawn({
        let client = client.clone();
        async move { client.workspace_symbol("fast".into()).await }
    });
    let fast_request = read_client_frame(&mut reader).await;
    assert_ne!(slow_request["id"], fast_request["id"]);
    send_server_frame(
        &mut writer,
        json!({"jsonrpc":"2.0","id":fast_request["id"],"result":["fast"]}),
    )
    .await;
    assert_eq!(fast.await.unwrap().unwrap(), json!(["fast"]));
    assert!(
        !slow.is_finished(),
        "fast answer did not wait for slow request"
    );
    tokio::time::advance(Duration::from_millis(45)).await;
    slow.abort();
    assert!(slow.await.unwrap_err().is_cancelled());
    let cancelled = read_client_frame(&mut reader).await;
    assert_eq!(cancelled["method"], "$/cancelRequest");
    assert_eq!(cancelled["params"]["id"], slow_request["id"]);
    assert!(!client.is_busy());
    assert!(client.is_alive().await);
}

#[tokio::test(start_paused = true)]
async fn cancelling_readiness_preserves_the_live_client_for_a_later_request() {
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    client.inner.progress.on_begin("index".into()).await;
    let result = tokio::time::timeout(
        Duration::from_millis(75),
        client.wait_for_ready(Some(5_000)),
    )
    .await;
    assert!(result.is_err());
    assert!(client.is_alive().await);
    client.inner.progress.on_end("index").await;
    assert_eq!(
        client.wait_for_ready(Some(500)).await.unwrap(),
        "progressIdle"
    );
    let server = tokio::spawn(async move {
        let request = read_client_frame(&mut reader).await;
        send_server_frame(
            &mut writer,
            json!({"jsonrpc":"2.0","id":request["id"],"result":[]}),
        )
        .await;
        (reader, writer)
    });
    assert_eq!(
        client.workspace_symbol("after".into()).await.unwrap(),
        json!([])
    );
    let _server = server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn retrying_requests_keep_one_caller_deadline() {
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let server = tokio::spawn(async move {
        let first = read_client_frame(&mut reader).await;
        send_server_frame(
            &mut writer,
            json!({"jsonrpc":"2.0","id":first["id"],"error":{"code":-32801,"message":"changed"}}),
        )
        .await;
        let second = read_client_frame(&mut reader).await;
        assert_eq!(second["method"], first["method"]);
        let cancelled = read_client_frame(&mut reader).await;
        assert_eq!(cancelled["method"], "$/cancelRequest");
        (reader, writer)
    });
    let started = Instant::now();
    let result = crate::lsp::with_caller_deadline(
        started + Duration::from_millis(2_000),
        client.workspace_symbol("retry".into()),
    )
    .await;
    assert!(result.is_err());
    assert!(started.elapsed() <= Duration::from_millis(2_000));
    let _server = server.await.unwrap();
}
