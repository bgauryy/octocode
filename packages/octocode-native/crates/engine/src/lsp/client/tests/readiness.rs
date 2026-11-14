use super::document_sync::{
    client_with_fake_server_using, read_client_frame, send_server_frame as send,
};
use super::*;
use serde_json::json;
use tokio::io::DuplexStream;
use tokio::time::Instant;

async fn progress(writer: &mut DuplexStream, kind: &str) {
    send(writer, json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":"load","value":{"kind":kind}}})).await;
}

#[tokio::test(start_paused = true)]
async fn first_open_waits_for_the_project_load_the_open_triggers() {
    let tmp = TmpDir::new();
    let source = tmp.0.join("a.ts");
    let content = "function foo() {}\nfoo();\n";
    std::fs::write(&source, content).expect("source");
    let path = source.to_string_lossy().into_owned();
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    assert_eq!(
        client
            .wait_for_ready(Some(300))
            .await
            .expect("silent startup"),
        Readiness::SilentServer
    );
    let server = tokio::spawn(async move {
        assert_eq!(
            read_client_frame(&mut reader).await["method"],
            "textDocument/didOpen"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        progress(&mut writer, "begin").await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        progress(&mut writer, "end").await;
        let request = read_client_frame(&mut reader).await;
        assert_eq!(request["method"], "textDocument/references");
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","id":request["id"],"result":[{
                "uri":request["params"]["textDocument"]["uri"],
                "range":{"start":{"line":0,"character":9},"end":{"line":0,"character":12}}
            }]}),
        )
        .await;
        (reader, writer)
    });
    let started = Instant::now();
    let readiness = client
        .open_document_and_wait(path.clone(), content, Some(400), Some(5_000))
        .await
        .expect("open");
    assert_eq!(readiness, Some(Readiness::ProgressIdle));
    assert_eq!(started.elapsed(), Duration::from_millis(700));
    let references = client
        .get_locations(
            LocationRequest::References {
                include_declaration: true,
            },
            path.clone(),
            0,
            9,
            &SnippetReadPolicy::default(),
            None,
        )
        .await
        .expect("references");
    assert_eq!(
        references.len(),
        1,
        "the open-triggered load must finish first"
    );
    let (_reader, _writer) = server.await.expect("mock server");
    let started = Instant::now();
    assert_eq!(
        client
            .open_document_and_wait(path, content, Some(400), Some(5_000))
            .await
            .expect("resync"),
        None
    );
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "unchanged document does not settle again"
    );
}

#[tokio::test(start_paused = true)]
async fn readiness_timeout_client_waits_again_without_restarting() {
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let connection = client.connection_handle().await.expect("connection");
    let mut active = client.inner.progress.subscribe();
    progress(&mut writer, "begin").await;
    active
        .wait_for(|count| *count == 1)
        .await
        .expect("indexing began");
    assert_eq!(
        client
            .wait_for_ready(Some(2_000))
            .await
            .expect("first wait"),
        Readiness::Timeout
    );
    let server = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1_000)).await;
        progress(&mut writer, "end").await;
        let request = read_client_frame(&mut reader).await;
        assert_eq!(request["method"], "textDocument/hover");
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","id":request["id"],"result":{"contents":"ready"}}),
        )
        .await;
        (reader, writer)
    });
    let started = Instant::now();
    client.resume_timed_out_readiness(2_000).await;
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(1_200),
        "remaining indexing plus quiescence, no second startup settle"
    );
    assert_eq!(client.readiness(), Some(Readiness::ProgressIdle));
    assert!(Arc::ptr_eq(
        &connection,
        &client.connection_handle().await.expect("same connection")
    ));
    let tmp = TmpDir::new();
    client
        .get_hover(tmp.0.join("a.ts").to_string_lossy().into_owned(), 0, 0)
        .await
        .expect("resumed client answers");
    let (_reader, _writer) = server.await.expect("mock server");
    let started = Instant::now();
    client.resume_timed_out_readiness(2_000).await;
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "ready client returns without another wait"
    );
}

#[tokio::test(start_paused = true)]
async fn first_open_after_a_confirmed_project_load_skips_the_settle() {
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    client.inner.progress.on_begin("startup".into()).await;
    client.inner.progress.on_end("startup").await;
    assert_eq!(
        client.wait_for_ready(Some(5_000)).await.expect("ready"),
        Readiness::ProgressIdle
    );
    let tmp = TmpDir::new();
    let started = Instant::now();
    let readiness = client
        .open_document_and_wait(
            tmp.0.join("a.ts").to_string_lossy().into_owned(),
            "export {};\n",
            Some(400),
            Some(5_000),
        )
        .await
        .expect("open");
    assert_eq!(
        readiness,
        Some(Readiness::ProgressIdle),
        "a wait owed nothing keeps the startup wait's confirmation, not a silent-server label"
    );
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "no first-open settle stacked on confirmed startup"
    );
}

#[tokio::test(start_paused = true)]
async fn a_settle_cap_bounds_every_silent_server_wait() {
    let (client, mut reader, mut writer) = client_with_fake_server_using(
        NativeLspClient::with_options(
            ts_config(),
            LspClientOptions {
                settle_cap: Some(Duration::ZERO),
            },
        ),
        64 * 1024,
    )
    .await;
    let server = tokio::spawn(async move {
        assert_eq!(
            read_client_frame(&mut reader).await["method"],
            "textDocument/didOpen"
        );
        let request = read_client_frame(&mut reader).await;
        assert_eq!(request["method"], "textDocument/definition");
        send(
            &mut writer,
            json!({"jsonrpc":"2.0","id":request["id"],"result":[]}),
        )
        .await;
        (reader, writer)
    });
    let tmp = TmpDir::new();
    let path = tmp.0.join("a.ts").to_string_lossy().into_owned();
    std::fs::write(&path, "export {};\n").expect("source");
    let started = Instant::now();
    assert_eq!(
        client.wait_for_ready(Some(5_000)).await.expect("startup"),
        Readiness::SilentServer
    );
    assert_eq!(
        client
            .open_document_and_wait(path.clone(), "export {};\n", Some(400), Some(5_000))
            .await
            .expect("open"),
        Some(Readiness::SilentServer)
    );
    client
        .get_locations(
            LocationRequest::Definition,
            path,
            0,
            0,
            &SnippetReadPolicy::default(),
            Some(Duration::from_millis(400)),
        )
        .await
        .expect("definition");
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "all three settle paths honor the zero cap"
    );
    let (_reader, _writer) = server.await.expect("mock server");
}

#[tokio::test(start_paused = true)]
async fn a_load_ending_before_empty_judgment_invalidates_the_recorded_answer() {
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let connection = client.connection_handle().await.expect("connection");
    let server = tokio::spawn(async move {
        for answer in [json!([]), json!([{ "name": "loaded" }])] {
            let request = read_client_frame(&mut reader).await;
            assert_eq!(request["method"], "textDocument/documentSymbol");
            send(
                &mut writer,
                json!({"jsonrpc":"2.0","id":request["id"],"result":answer}),
            )
            .await;
        }
        (reader, writer)
    });
    let scope = ResponseScope::default();
    let answers = Arc::clone(scope.answers.as_ref().expect("selected answers"));
    let tmp = TmpDir::new();
    let path = tmp.0.join("a.ts").to_string_lossy().into_owned();
    RESPONSE_SCOPE
        .scope(std::cell::RefCell::new(scope), async {
            client.inner.progress.on_begin("load".into()).await;
            let asked = client.asked_at().await;
            let empty = client.get_document_symbols(path.clone()).await.unwrap();
            assert_eq!(*empty, json!([]));
            assert_eq!(answers.lock().unwrap().0.len(), 1);

            // The response was produced during loading, but its judgment runs
            // after the load ended. No new begin separates these two moments.
            client.inner.progress.on_end("load").await;
            assert_eq!(
                client.judge_empty_answer(asked, None).await,
                EmptyAnswer::AskAgain,
                "a loading-era empty answer cannot establish absence"
            );
            assert!(answers.lock().unwrap().0.is_empty());
            let loaded = client.get_document_symbols(path).await.unwrap();
            assert_eq!(*loaded, json!([{ "name": "loaded" }]));
        })
        .await;
    assert!(Arc::ptr_eq(
        &connection,
        &client.connection_handle().await.expect("same connection")
    ));
    let (_reader, _writer) = server.await.expect("two actual provider requests");
}

#[tokio::test(start_paused = true)]
async fn a_later_idle_judgment_cannot_validate_an_answer_asked_during_loading() {
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    client.inner.progress.on_begin("load".into()).await;
    let asked_while_loading = client.asked_at().await;
    client.inner.progress.on_end("load").await;

    assert_eq!(
        client
            .judge_empty_answer(client.asked_at().await, None)
            .await,
        EmptyAnswer::Stands,
        "a request made after loading sees an idle server"
    );
    assert_eq!(
        client.judge_empty_answer(asked_while_loading, None).await,
        EmptyAnswer::AskAgain,
        "another request's later idle observation cannot validate the earlier answer"
    );
}
