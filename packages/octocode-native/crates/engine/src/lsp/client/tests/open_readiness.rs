use super::document_sync::{
    client_with_fake_server_using, read_client_frame, send_server_frame as send,
};
use super::*;
use serde_json::json;

#[tokio::test(start_paused = true)]
async fn timed_out_document_open_keeps_waiting_until_project_progress_finishes() {
    let tmp = TmpDir::new();
    let path = tmp.0.join("a.ts").to_string_lossy().into_owned();
    let content = "export {};\n";
    std::fs::write(&path, content).expect("source");
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let connection = client.connection_handle().await.expect("connection");
    let (finish, finished) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        assert_eq!(
            read_client_frame(&mut reader).await["method"],
            "textDocument/didOpen"
        );
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","method":"$/progress","params":{
                "token":"project","value":{"kind":"begin"}
            }}),
        )
        .await;
        finished.await.expect("test releases project load");
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","method":"$/progress","params":{
                "token":"project","value":{"kind":"end"}
            }}),
        )
        .await;
        let request = read_client_frame(&mut reader).await;
        assert_eq!(
            request["method"], "textDocument/hover",
            "no duplicate didOpen"
        );
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","id":request["id"],"result":{"contents":"ready"}}),
        )
        .await;
        (reader, writer)
    });
    for attempt in 0..2 {
        assert_eq!(
            client
                .open_document_and_wait(path.clone(), content, Some(400), Some(1_000))
                .await
                .expect("sync while loading")
                .as_deref(),
            Some("timeout"),
            "attempt {attempt} must disclose the unfinished load of the cached document"
        );
    }
    finish.send(()).expect("release project load");
    assert_eq!(
        client
            .open_document_and_wait(path.clone(), content, Some(400), Some(1_000))
            .await
            .expect("sync after load")
            .as_deref(),
        Some("progressIdle")
    );
    assert_eq!(
        client
            .open_document_and_wait(path.clone(), content, Some(400), Some(1_000))
            .await
            .expect("settled cached document"),
        None,
        "only a completed readiness wait is memoized"
    );
    assert!(Arc::ptr_eq(
        &connection,
        &client.connection_handle().await.expect("same connection")
    ));
    client
        .get_hover(path, 0, 0)
        .await
        .expect("ready server answers");
    let (_reader, _writer) = server.await.expect("server");
}
