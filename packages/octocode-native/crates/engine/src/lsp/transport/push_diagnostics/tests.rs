use super::*;
use crate::test_support::clock::paused;

#[test]
fn push_diagnostics_rejects_an_older_document_version() {
    let store = PushDiagnosticsStore::new();
    store.record(json!({
        "uri": "file:///workspace/a.ts",
        "version": 2,
        "diagnostics": [{ "message": "stale" }]
    }));

    assert!(
        store
            .report("file:///workspace/a.ts", Some(3), false)
            .is_none()
    );
    assert_eq!(
        store
            .report("file:///workspace/a.ts", Some(2), false)
            .expect("matching version")["items"][0]["message"],
        "stale"
    );
}

#[test]
fn push_diagnostics_accepts_a_versionless_report_as_current() {
    // Servers such as typescript-language-server omit `version` from
    // publishDiagnostics. Every document sync clears the cached record
    // first, so a versionless record present afterwards was published
    // after the sync and is treated as current.
    let store = PushDiagnosticsStore::new();
    store.record(json!({
        "uri": "file:///workspace/a.ts",
        "diagnostics": [{ "message": "no-version" }]
    }));

    assert_eq!(
        store
            .report("file:///workspace/a.ts", Some(3), false)
            .expect("versionless report accepted for a min version")["items"][0]["message"],
        "no-version"
    );
    assert_eq!(
        store
            .report("file:///workspace/a.ts", None, false)
            .expect("versionless report readable without a min")["items"][0]["message"],
        "no-version"
    );
}

#[test]
fn push_diagnostics_preserves_the_complete_published_report() {
    let store = PushDiagnosticsStore::new();
    let diagnostics = (0..4097)
        .map(|index| json!({ "message": format!("diagnostic {index}: λ😀") }))
        .collect::<Vec<_>>();
    store.record(json!({
        "uri": "file:///workspace/a.ts",
        "version": 3,
        "diagnostics": diagnostics,
    }));

    let report = store
        .report("file:///workspace/a.ts", Some(3), false)
        .expect("published report");
    assert_eq!(report["items"], json!(diagnostics));
}

#[test]
fn push_diagnostics_preserves_a_large_indivisible_diagnostic() {
    let store = PushDiagnosticsStore::new();
    let diagnostic = json!({
        "message": "λ😀".repeat(128 * 1024),
        "data": { "source": "complete provider evidence" },
    });
    store.record(json!({
        "uri": "file:///workspace/a.ts",
        "diagnostics": [diagnostic],
    }));

    let report = store
        .report("file:///workspace/a.ts", None, false)
        .expect("published report");
    assert_eq!(report["items"], json!([diagnostic]));
}

#[test]
fn push_diagnostics_evicts_whole_reports_under_the_shared_memory_budget() {
    static BUDGET: MemoryBudget = MemoryBudget::new(|| 16 * 1024);
    let store = PushDiagnosticsStore::with_budget(&BUDGET);
    let competing = Memo::<String, Vec<u8>>::new(&BUDGET);
    let diagnostics = json!([{ "message": "λ東京".repeat(256) }]);
    for index in 0..32 {
        store.record(json!({
            "uri": format!("file:///w/{index}.ts"),
            "diagnostics": diagnostics
        }));
        assert!(BUDGET.used() <= BUDGET.limit());
    }
    assert!(store.report("file:///w/0.ts", None, false).is_none());
    assert_eq!(
        store
            .report("file:///w/31.ts", None, false)
            .expect("newest retained")["items"],
        diagnostics
    );
    // Other engine stores compete against the same owner; no diagnostic
    // store can keep a second, unaccounted copy outside its budget.
    competing.insert(
        "other tool".into(),
        vec![0; BUDGET.limit()],
        BUDGET.limit(),
        None,
    );
    assert!(store.report("file:///w/31.ts", None, false).is_none());
    assert!(BUDGET.used() <= BUDGET.limit());
    drop((store, competing));
    assert_eq!(BUDGET.used(), 0);
}

#[test]
fn an_unretainable_report_is_unavailable_instead_of_clean_or_stale() {
    paused(|clock| async move {
        static BUDGET: MemoryBudget = MemoryBudget::new(|| 4 * 1024);
        let store = PushDiagnosticsStore::with_budget(&BUDGET);
        let uri = "file:///w/oversized.ts";
        store.record(json!({ "uri": uri, "diagnostics": [] }));
        assert!(store.report(uri, None, false).is_some());
        store.record(json!({
            "uri": uri,
            "diagnostics": [{ "message": "λ東京".repeat(BUDGET.limit()) }]
        }));
        assert!(
            store.report(uri, None, false).is_none(),
            "old clean report was replaced"
        );
        assert!(
            !store
                .wait_for_all(&[(uri.to_owned(), None)], clock.now(), &clock)
                .await
                && store.report(uri, None, true).is_none(),
            "no invented empty report"
        );
        assert_eq!(BUDGET.used(), 0);
    });
}

#[test]
fn push_diagnostics_ignores_notifications_for_other_documents() {
    paused(|clock| async move {
        let store = PushDiagnosticsStore::new();
        let waiter_store = Arc::clone(&store);
        let task_clock = clock.clone();
        let waiter = tokio::spawn(async move {
            let until = task_clock.now() + std::time::Duration::from_secs(1);
            waiter_store
                .wait_for_all(
                    &[("file:///workspace/a.ts".to_owned(), Some(3))],
                    until,
                    &task_clock,
                )
                .await
                .then(|| waiter_store.report("file:///workspace/a.ts", Some(3), false))
                .flatten()
        });
        tokio::task::yield_now().await;

        store.record(json!({
            "uri": "file:///workspace/b.ts",
            "version": 3,
            "diagnostics": [{ "message": "other" }]
        }));
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());

        store.record(json!({
            "uri": "file:///workspace/a.ts",
            "version": 3,
            "diagnostics": [{ "message": "target" }]
        }));
        let report = waiter
            .await
            .expect("wait task")
            .expect("target diagnostics report");
        assert_eq!(report["items"][0]["message"], "target");
    });
}
