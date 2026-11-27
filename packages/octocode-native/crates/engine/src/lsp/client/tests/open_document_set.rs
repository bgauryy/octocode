//! Open-document bookkeeping and the busy lease.

use super::*;
use crate::lsp::client::open_documents::OpenDocuments;

#[test]
fn open_documents_evicts_least_recently_used_when_over_cap() {
    // A long session must not grow `open_docs` (and the server-side document
    // set) without bound. Over the cap, the least-recently-synced document is
    // evicted and its URI is returned so the caller can emit a `didClose`.
    let mut docs = OpenDocuments::new(2);

    let (version_a, evicted) = docs.reserve("file:///a");
    assert_eq!(version_a, 1);
    assert!(evicted.is_none());

    let (_, evicted) = docs.reserve("file:///b");
    assert!(evicted.is_none());
    assert_eq!(docs.len(), 2);

    // Touch `a` so `b` becomes the least-recently-used document.
    let (version_a2, evicted) = docs.reserve("file:///a");
    assert_eq!(version_a2, 2, "a re-open bumps the document version");
    assert!(evicted.is_none());

    // Opening a third distinct document exceeds the cap: `b` (LRU) is evicted
    // and returned as the didClose target; the map stays bounded.
    let (version_c, evicted) = docs.reserve("file:///c");
    assert_eq!(version_c, 1);
    assert_eq!(
        evicted.as_deref(),
        Some("file:///b"),
        "the least-recently-used document is the eviction/didClose target"
    );
    assert_eq!(docs.len(), 2, "the open-document map is bounded by the cap");
    assert_eq!(docs.version("file:///b"), None);
    assert_eq!(docs.version("file:///a"), Some(2));
    assert_eq!(docs.version("file:///c"), Some(1));

    // The currently-syncing document is never chosen as its own eviction victim.
    let (_, evicted) = docs.reserve("file:///c");
    assert_ne!(evicted.as_deref(), Some("file:///c"));
}

#[test]
fn open_documents_rollback_restores_prior_version_state() {
    let mut docs = OpenDocuments::new(4);
    // A failed first sync (didOpen) rolls the URI back out entirely.
    let (v1, _) = docs.reserve("file:///a");
    docs.rollback("file:///a", v1);
    assert_eq!(docs.version("file:///a"), None);

    // A failed later sync (didChange) rolls the version back by one.
    docs.reserve("file:///b");
    let (v2, _) = docs.reserve("file:///b");
    assert_eq!(v2, 2);
    docs.rollback("file:///b", v2);
    assert_eq!(docs.version("file:///b"), Some(1));
}

#[test]
fn lease_marks_the_client_busy_until_every_holder_releases() {
    let client = NativeLspClient::new(ts_config());
    assert!(!client.is_busy());
    let first = client.lease();
    let clone = client.clone();
    let second = clone.lease();
    assert!(client.is_busy() && clone.is_busy());
    drop(first);
    assert!(client.is_busy(), "one lease still held");
    // Owned + 'static: a lease can move into another task.
    let moved = std::thread::spawn(move || drop(second));
    moved.join().unwrap();
    assert!(!client.is_busy());
}

#[test]
fn open_documents_remember_the_synced_content() {
    let mut docs = OpenDocuments::new(2);
    let (version, _) = docs.reserve("file:///a");
    assert_eq!(docs.unchanged("file:///a", 7), None, "nothing synced yet");
    docs.record("file:///a", version, "/a", 7);
    assert_eq!(
        docs.unchanged("file:///a", 7),
        Some(1),
        "same content: no resync"
    );
    assert_eq!(
        docs.unchanged("file:///a", 8),
        None,
        "edited content resyncs"
    );
    // A new sync forgets the old content until it is recorded.
    let (version, _) = docs.reserve("file:///a");
    assert_eq!(docs.unchanged("file:///a", 7), None);
    docs.record("file:///a", version, "/a", 8);
    assert_eq!(docs.unchanged("file:///a", 8), Some(2));
    // A failed sync or an eviction forgets the content too.
    let (version, _) = docs.reserve("file:///a");
    docs.rollback("file:///a", version);
    assert_eq!(docs.unchanged("file:///a", 8), None);
    docs.record("file:///a", 2, "/a", 8);
    docs.reserve("file:///b");
    docs.reserve("file:///c");
    assert_eq!(docs.unchanged("file:///a", 8), None, "evicted");
}
