use super::*;
use crate::lsp::types::LspLanguageServerConfig;

fn client() -> NativeLspClient {
    NativeLspClient::new(LspLanguageServerConfig {
        command: "server".into(),
        args: None,
        workspace_root: "/w".into(),
        language_id: Some("go".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    })
}

#[test]
fn snapshots_name_each_change() {
    let first = BTreeMap::from([
        ("a.go".to_owned(), "1".to_owned()),
        ("b.go".to_owned(), "1".to_owned()),
    ]);
    let exact = |_: &str, before: &str, after: &str| before == after;
    let second = BTreeMap::from([
        ("a.go".to_owned(), "2".to_owned()),
        ("c.go".to_owned(), "1".to_owned()),
    ]);
    let mut changes = changed_files(&first, &second, exact);
    changes.sort();
    assert_eq!(
        changes,
        vec![
            ("a.go".to_owned(), CHANGED),
            ("b.go".to_owned(), DELETED),
            ("c.go".to_owned(), CREATED),
        ]
    );
    assert_eq!(changed_files(&second, &second, exact), Vec::new());
    // Stamps the caller proves the same file owe no event.
    let restamped = BTreeMap::from([
        ("a.go".to_owned(), "2'".to_owned()),
        ("c.go".to_owned(), "1".to_owned()),
    ]);
    let same_a = |path: &str, before: &str, after: &str| {
        before == after || (path == "a.go" && after == "2'")
    };
    assert_eq!(changed_files(&second, &restamped, same_a), Vec::new());
}

#[tokio::test]
async fn failed_notifications_keep_the_previous_snapshot() {
    let client = client();
    let root = crate::test_support::TmpDir::new();
    let first = BTreeMap::from([("a.rs".to_owned(), "before".to_owned())]);
    let unchanged = |_: &str, before: &str, after: &str| before == after;
    *client.inner.watched_files.lock().expect("snapshot") = Some(first.clone());
    let next = BTreeMap::from([("a.rs".to_owned(), "after".to_owned())]);
    assert!(
        client
            .sync_watched_files(|previous| async move {
                WatchedFileUpdate::new(&root, previous.as_ref(), next, unchanged)
            })
            .await
            .is_err()
    );
    assert_eq!(
        client
            .inner
            .watched_files
            .lock()
            .expect("snapshot")
            .as_ref(),
        Some(&first),
        "a missing connection must not acknowledge the unsent change"
    );
}
