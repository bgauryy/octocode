//! The open settle is paid once per project, not once per file.

use super::document_sync::{
    client_with_fake_server_using, read_client_frame, send_server_frame as send,
};
use super::*;
use serde_json::json;
use tokio::io::{BufReader, DuplexStream};
use tokio::time::Instant;

async fn progress(writer: &mut DuplexStream, token: &str, kind: &str) {
    send(writer, json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":token,"value":{"kind":kind}}})).await;
}

/// `tsconfig.json` projects `names` under one temp dir, each with `a.ts`
/// and `b.ts`.
fn projects(names: &[&str]) -> (TmpDir, Vec<PathBuf>) {
    let tmp = TmpDir::new();
    let dirs = names
        .iter()
        .map(|name| {
            let dir = tmp.0.join(name);
            std::fs::create_dir_all(&dir).expect("project dir");
            std::fs::write(dir.join("tsconfig.json"), "{}").expect("tsconfig");
            dir
        })
        .collect();
    (tmp, dirs)
}

fn file(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// Open `path` and return how long the open waited.
async fn open(client: &NativeLspClient, path: String) -> Duration {
    let started = Instant::now();
    client
        .open_document_and_wait(path, "export {};\n", Some(400), Some(5_000))
        .await
        .expect("open")
        .expect("first open");
    started.elapsed()
}

/// A server that answers the first `didOpen` with a 400 ms project load.
async fn load_on_first_open(
    mut reader: BufReader<DuplexStream>,
    mut writer: DuplexStream,
) -> (BufReader<DuplexStream>, DuplexStream) {
    assert_eq!(
        read_client_frame(&mut reader).await["method"],
        "textDocument/didOpen"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    progress(&mut writer, "load", "begin").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    progress(&mut writer, "load", "end").await;
    (reader, writer)
}

#[tokio::test(start_paused = true)]
async fn a_later_file_of_a_loaded_project_opens_without_a_settle() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, reader, writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let server = tokio::spawn(load_on_first_open(reader, writer));
    assert_eq!(
        open(&client, file(&dirs[0], "a.ts")).await,
        Duration::from_millis(700),
        "the first open waits for the load it triggers"
    );
    let (_reader, _writer) = server.await.expect("mock server");
    assert_eq!(
        open(&client, file(&dirs[0], "b.ts")).await,
        Duration::ZERO,
        "the loaded project starts no new load"
    );
}

#[tokio::test(start_paused = true)]
async fn the_first_open_of_another_project_still_waits() {
    let (_tmp, dirs) = projects(&["p", "q"]);
    let (client, reader, writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let server = tokio::spawn(load_on_first_open(reader, writer));
    open(&client, file(&dirs[0], "a.ts")).await;
    let (_reader, _writer) = server.await.expect("mock server");
    assert_eq!(
        open(&client, file(&dirs[1], "a.ts")).await,
        Duration::from_millis(400),
        "a second project settles for its own load"
    );
}

#[tokio::test(start_paused = true)]
async fn a_silent_project_is_remembered() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    assert_eq!(
        open(&client, file(&dirs[0], "a.ts")).await,
        Duration::from_millis(400)
    );
    assert_eq!(open(&client, file(&dirs[0], "b.ts")).await, Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn a_load_no_open_waited_for_makes_the_next_open_wait() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, _reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    open(&client, file(&dirs[0], "a.ts")).await;
    // The server reloads the project on its own (a tsconfig edit).
    let mut seen = client.inner.progress.subscribe();
    progress(&mut writer, "reload", "begin").await;
    progress(&mut writer, "reload", "end").await;
    seen.wait_for(|count| *count == 0)
        .await
        .expect("reload ended");
    while client.inner.progress.begins() == 0 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        open(&client, file(&dirs[0], "b.ts")).await,
        Duration::from_millis(400),
        "the reload is not known to be finished for this file"
    );
}

#[tokio::test(start_paused = true)]
async fn a_new_server_process_forgets_loaded_projects() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    open(&client, file(&dirs[0], "a.ts")).await;
    client.reset_session_state();
    assert_eq!(
        open(&client, file(&dirs[0], "b.ts")).await,
        Duration::from_millis(400)
    );
}

#[tokio::test(start_paused = true)]
async fn an_explicit_settle_is_also_owed_once_per_project() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    open(&client, file(&dirs[0], "a.ts")).await;
    let started = Instant::now();
    client
        .open_document_and_wait(file(&dirs[0], "b.ts"), "export {};\n", Some(200), None)
        .await
        .expect("open");
    assert_eq!(started.elapsed(), Duration::ZERO);
}

/// A document opened without the wait (an outline) still owes it to the
/// first caller that waits: its project may not have loaded.
#[tokio::test(start_paused = true)]
async fn a_document_opened_without_the_wait_owes_it_in_an_unsettled_project() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let path = file(&dirs[0], "a.ts");
    client
        .open_document(path.clone(), "export {};\n")
        .await
        .expect("outline open");
    assert_eq!(
        open(&client, path.clone()).await,
        Duration::from_millis(400)
    );
    let started = Instant::now();
    assert_eq!(
        client
            .open_document_and_wait(path, "export {};\n", Some(400), Some(5_000))
            .await
            .expect("again"),
        None,
        "the wait is owed once per document"
    );
    assert_eq!(started.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn a_document_opened_without_the_wait_in_a_settled_project_waits_nothing() {
    let (_tmp, dirs) = projects(&["p"]);
    let (client, _reader, _writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    open(&client, file(&dirs[0], "a.ts")).await;
    let path = file(&dirs[0], "b.ts");
    client
        .open_document(path.clone(), "export {};\n")
        .await
        .expect("outline open");
    assert_eq!(open(&client, path).await, Duration::ZERO);
}
