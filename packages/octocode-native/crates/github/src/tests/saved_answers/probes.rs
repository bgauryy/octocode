//! Whether a saved answer would serve is decided from the store's probe: the
//! question loads no body, a body over the request's limit is asked for
//! again, and only a read that serves the answer loads it.
use super::proven_commits::read;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A store that probes as a disk store does (the answer without its body,
/// its length as set) and counts its loads and probes.
#[derive(Default)]
struct Counting {
    memory: Memory,
    loads: AtomicUsize,
    probes: AtomicUsize,
    length: Mutex<Option<usize>>,
}

impl SavedAnswers for Counting {
    fn load(&self, scope: &str, request: &str) -> Option<SavedAnswer> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        self.memory.load(scope, request)
    }
    fn probe(&self, scope: &str, request: &str) -> Option<(SavedAnswer, Option<usize>)> {
        self.probes.fetch_add(1, Ordering::SeqCst);
        let mut saved = self.memory.load(scope, request)?;
        saved.answer.body = AnswerBody::default();
        Some((saved, *self.length.lock().unwrap()))
    }
    fn save(&self, scope: &str, request: &str, saved: SavedAnswer) {
        self.memory.save(scope, request, saved);
    }
    fn forget(&self, scope: &str, request: &str) {
        self.memory.forget(scope, request);
    }
}

#[tokio::test]
async fn a_saved_answer_is_probed_to_decide_and_loaded_only_to_serve() {
    let server = MockServer::start().await;
    let sha = "c".repeat(40);
    Mock::given(path(format!("/api/v3/repos/a/b/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"content\":\"\"}"))
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let mut transport = anonymous_transport(&endpoint);
    let store = Arc::new(Counting::default());
    transport.set_saved_answers(Some(store.clone()));
    let spec = read(&endpoint, &["git", "blobs", &sha]);
    let context = request("t");
    let first = transport.execute(spec.clone(), &context).await.unwrap();
    let loads = store.loads.load(Ordering::SeqCst);
    let probes = store.probes.load(Ordering::SeqCst);

    assert!(transport.answers_unasked(&spec, &context), "a pinned blob");
    assert_eq!(
        store.loads.load(Ordering::SeqCst),
        loads,
        "the question loads nothing"
    );
    assert_eq!(store.probes.load(Ordering::SeqCst), probes + 1);

    *store.length.lock().unwrap() = Some(context.budget.max_body_bytes + 1);
    assert!(
        !transport.answers_unasked(&spec, &context),
        "a body over the request's limit is asked for"
    );
    *store.length.lock().unwrap() = Some(context.budget.max_body_bytes);
    assert!(transport.answers_unasked(&spec, &context));

    let again = transport.execute(spec, &context).await.unwrap();
    assert_eq!(again.body, first.body);
    assert_eq!(requests(&server).await.len(), 1, "served unasked");
    assert!(store.loads.load(Ordering::SeqCst) > loads, "serving loads");
}
