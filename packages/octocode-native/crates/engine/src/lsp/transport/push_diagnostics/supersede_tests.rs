use super::*;
use crate::test_support::clock::paused;

/// A file event can change what any document's diagnostics say (a new
/// module resolves another file's import), so every report predates it.
#[test]
fn a_workspace_change_supersedes_every_report() {
    let store = PushDiagnosticsStore::new();
    for uri in ["file:///w/a.ts", "file:///w/b.ts"] {
        store.record(json!({ "uri": uri, "diagnostics": [{ "message": "before" }] }));
    }
    store.supersede_all();
    assert!(store.report("file:///w/a.ts", None, false).is_none());
    assert!(store.report("file:///w/b.ts", None, false).is_none());
    store.record(json!({ "uri": "file:///w/a.ts", "diagnostics": [] }));
    assert_eq!(
        store.report("file:///w/a.ts", None, false).expect("fresh")["superseded"],
        false
    );
}

/// A document change keeps the server's last report, marked: a reader
/// prefers a fresh publish and falls back to it after its wait.
#[test]
fn a_superseded_report_stands_in_only_after_the_wait() {
    paused(|clock| async move {
        let store = PushDiagnosticsStore::new();
        store.record(json!({
            "uri": "file:///w/a.ts",
            "diagnostics": [{ "message": "before" }]
        }));
        store.supersede("file:///w/a.ts");
        assert!(store.report("file:///w/a.ts", None, false).is_none());
        let until = clock.now() + std::time::Duration::from_millis(50);
        assert!(
            !store
                .wait_for_all(&[("file:///w/a.ts".to_owned(), None)], until, &clock)
                .await,
            "no fresh report"
        );
        let fallback = store
            .report("file:///w/a.ts", None, true)
            .expect("the superseded report stands in");
        assert_eq!(fallback["superseded"], true);
        assert_eq!(fallback["items"][0]["message"], "before");
        // A fresh publish replaces it, unflagged.
        store.record(json!({
            "uri": "file:///w/a.ts",
            "diagnostics": []
        }));
        let fresh = store
            .report("file:///w/a.ts", None, false)
            .expect("fresh report");
        assert_eq!(fresh["superseded"], false);
        assert_eq!(fresh["items"], json!([]));
    });
}
