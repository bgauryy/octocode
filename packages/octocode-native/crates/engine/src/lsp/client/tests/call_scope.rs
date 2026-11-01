//! One tool call's requests: answers reused within the call, open documents
//! compared with their files once per call, and pull diagnostics that name
//! the previous report.

use super::document_sync::{client_with_fake_server_using, read_client_frame, send_server_frame};
use super::*;
use serde_json::json;
use tokio::sync::mpsc;

/// A fake server that reports every client frame's method on `seen` and
/// answers each request with `answer(request, nth request)`.
fn serve(
    mut reader: tokio::io::BufReader<tokio::io::DuplexStream>,
    mut writer: tokio::io::DuplexStream,
    answer: impl Fn(&Value, usize) -> Value + Send + 'static,
) -> (tokio::task::JoinHandle<()>, mpsc::UnboundedReceiver<Value>) {
    let (seen, frames) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut requests = 0;
        loop {
            let frame = read_client_frame(&mut reader).await;
            // Reported before the answer, so a caller that has its answer
            // sees every frame sent before its request.
            if seen.send(frame.clone()).is_err() {
                return;
            }
            if frame.get("id").is_some() {
                requests += 1;
                let result = answer(&frame, requests);
                send_server_frame(
                    &mut writer,
                    json!({"jsonrpc":"2.0","id":frame["id"],"result":result}),
                )
                .await;
            }
        }
    });
    (task, frames)
}

fn methods(frames: &mut mpsc::UnboundedReceiver<Value>) -> Vec<String> {
    std::iter::from_fn(|| frames.try_recv().ok())
        .filter_map(|frame| frame["method"].as_str().map(str::to_owned))
        .collect()
}

fn line_at(request: &Value, line: usize) -> Value {
    json!([{
        "uri": request["params"]["textDocument"]["uri"],
        "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 1}}
    }])
}

fn scope(reuse: bool, call: Option<CallId>) -> ResponseScope {
    ResponseScope {
        reuse,
        generation: "anchor".to_owned(),
        call,
    }
}

async fn definition_line(client: &NativeLspClient, path: &str, scope: ResponseScope) -> u32 {
    RESPONSE_SCOPE
        .scope(
            std::cell::RefCell::new(scope),
            client.get_locations(
                LocationRequest::Definition,
                path.to_owned(),
                0,
                0,
                &SnippetReadPolicy::default(),
                None,
            ),
        )
        .await
        .expect("definition")[0]
        .range
        .start
        .line
}

#[tokio::test(start_paused = true)]
async fn one_call_asks_the_server_once_per_distinct_request() {
    let tmp = TmpDir::new();
    let path = tmp.0.join("a.ts").to_string_lossy().into_owned();
    std::fs::write(&path, "x\n".repeat(8)).expect("source");
    let (client, reader, writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let (server, mut frames) = serve(reader, writer, line_at);
    let call = Some(CallId::next());
    let first = definition_line(&client, &path, scope(false, call)).await;
    assert_eq!(
        definition_line(&client, &path, scope(false, call)).await,
        first,
        "the same call reuses its own answer without scope-wide reuse"
    );
    assert_eq!(methods(&mut frames).len(), 1);
    let next_call = definition_line(&client, &path, scope(false, Some(CallId::next()))).await;
    assert_ne!(next_call, first, "another call asks again");
    let outside = definition_line(&client, &path, scope(false, None)).await;
    assert_ne!(outside, next_call, "no call: every request asks");
    server.abort();
}

#[tokio::test(start_paused = true)]
async fn open_documents_are_compared_with_their_files_once_per_call() {
    let tmp = TmpDir::new();
    let anchor = tmp.0.join("a.ts").to_string_lossy().into_owned();
    let other = tmp.0.join("b.ts").to_string_lossy().into_owned();
    std::fs::write(&anchor, "x\n").expect("anchor");
    std::fs::write(&other, "old\n").expect("other");
    let (client, reader, writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let (server, mut frames) = serve(reader, writer, line_at);
    client
        .open_document(other.clone(), "old\n")
        .await
        .expect("open");
    let call = Some(CallId::next());
    definition_line(&client, &anchor, scope(false, call)).await;
    std::fs::write(&other, "new text\n").expect("edit");
    definition_line(&client, &anchor, scope(false, call)).await;
    assert!(
        !methods(&mut frames).contains(&"textDocument/didChange".to_owned()),
        "files do not change under one call"
    );
    definition_line(&client, &anchor, scope(false, Some(CallId::next()))).await;
    assert_eq!(
        methods(&mut frames),
        ["textDocument/didChange", "textDocument/definition"],
        "the next call syncs the edit before it asks"
    );
    server.abort();
}

#[tokio::test(start_paused = true)]
async fn pull_diagnostics_name_the_previous_report_and_keep_it_when_unchanged() {
    let tmp = TmpDir::new();
    let path = tmp.0.join("a.ts").to_string_lossy().into_owned();
    std::fs::write(&path, "x\n").expect("source");
    let (client, reader, writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let (server, mut frames) = serve(reader, writer, |_, nth| match nth {
        1 => json!({"kind": "full", "resultId": "r1", "items": [{"message": "x"}]}),
        _ => json!({"kind": "unchanged", "resultId": format!("r{nth}")}),
    });
    client
        .open_document(path.clone(), "x\n")
        .await
        .expect("open");
    let first = client.get_diagnostics(path.clone()).await.expect("first");
    assert_eq!(first["items"], json!([{"message": "x"}]));
    let second = client.get_diagnostics(path.clone()).await.expect("second");
    assert_eq!(second["kind"], "full", "{second}");
    assert_eq!(
        second["items"], first["items"],
        "unchanged keeps the report"
    );
    assert_eq!(second["resultId"], "r2");
    client.get_diagnostics(path).await.expect("third");
    let previous: Vec<Value> = std::iter::from_fn(|| frames.try_recv().ok())
        .filter(|frame| frame["method"] == "textDocument/diagnostic")
        .map(|frame| frame["params"]["previousResultId"].clone())
        .collect();
    assert_eq!(previous, [Value::Null, json!("r1"), json!("r2")]);
    server.abort();
}
