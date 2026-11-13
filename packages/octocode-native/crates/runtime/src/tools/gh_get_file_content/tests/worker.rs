//! Synchronous content preparation must not occupy the asynchronous worker.
use super::*;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

#[derive(Clone, Default)]
struct ThreadScan(Arc<Mutex<Vec<ThreadId>>>);

impl ContentScan for ThreadScan {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        self.0
            .lock()
            .expect("record scanning thread")
            .push(std::thread::current().id());
        Ok((text.to_owned(), Vec::new()))
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unclassified_content_is_shaped_away_from_the_async_worker() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/plain-worker.txt",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode("worker-bound source\n")}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query = serde_json::from_value(json!({
        "owner":"a", "repo":"b", "path":"plain-worker.txt", "ref":sha
    }))
    .expect("query");
    let main_thread = std::thread::current().id();
    let scan = ThreadScan::default();
    let selection = shaping(crate::tools::test_support::selection_fake::SelectAll).selection;
    let result = execute(
        &provider,
        &query,
        &default_context(),
        Some(50_000),
        Shaping {
            security: scan.clone(),
            context: selection.context.clone(),
            selection,
        },
        &mut None,
    )
    .await
    .expect("read");
    assert_eq!(result.file.content.status, ReadStatus::Success);
    let scanning_threads = scan.0.lock().expect("scanning threads");
    assert!(!scanning_threads.is_empty(), "source was scanned");
    assert!(
        scanning_threads.iter().all(|id| *id != main_thread),
        "plain content shaping must run on a blocking worker"
    );
}
