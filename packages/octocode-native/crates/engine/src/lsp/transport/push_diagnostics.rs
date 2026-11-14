//! Push-diagnostics cache for a JSON-RPC LSP connection.
//!
//! Records complete `textDocument/publishDiagnostics` notifications the server
//! pushes unprompted against the process's shared memory budget, and lets callers
//! wait for a report at (or above) a requested document version. The transport
//! admits each whole notification before this store sees it; pagination belongs
//! to the caller, so this store must never discard a report's suffix.

use octocode_io::memo::{Memo, MemoryBudget};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use tokio::time::{Instant, timeout_at};

struct PushDiagnosticsRecord {
    params: Value,
    /// The document changed on the server after this report. A server that
    /// publishes only when its diagnostics change (typescript-language-server)
    /// never replaces it, so after a wait for a fresh report it still
    /// stands, flagged.
    superseded: AtomicBool,
}

pub(super) struct PushDiagnosticsStore {
    records: Memo<String, PushDiagnosticsRecord>,
    changed: Notify,
}

impl Default for PushDiagnosticsStore {
    fn default() -> Self {
        Self::with_budget(&crate::memory::MEMORY)
    }
}

impl PushDiagnosticsStore {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn with_budget(budget: &'static MemoryBudget) -> Self {
        Self {
            records: Memo::new(budget),
            changed: Notify::new(),
        }
    }

    pub(super) fn record(&self, params: Value) {
        let Some(uri) = params.get("uri").and_then(Value::as_str).map(str::to_owned) else {
            return;
        };
        let weight = std::mem::size_of::<PushDiagnosticsRecord>()
            .saturating_add(std::mem::size_of::<String>())
            .saturating_add(uri.len())
            .saturating_add(crate::json_size::heap_bytes(&params));
        // Admission and whole-entry eviction belong to the process budget.
        // An unretained report is unavailable, never an empty clean report.
        self.records.insert(
            uri,
            PushDiagnosticsRecord {
                params,
                superseded: AtomicBool::new(false),
            },
            weight,
            None,
        );
        self.changed.notify_waiters();
    }

    /// The document's text changed on the server: its report, if any, is
    /// kept but marked, so a reader prefers a fresh publish and falls back
    /// to it only once none arrived in time.
    pub(super) fn supersede(&self, uri: &str) {
        if let Some(record) = self.records.get(uri) {
            record.value.superseded.store(true, Ordering::Relaxed);
        }
    }

    /// A file changed on disk, and the server was told: any document's report
    /// may now be out of date, as a new module can resolve another file's
    /// import.
    pub(super) fn supersede_all(&self) {
        for uri in self.records.contents().0 {
            self.supersede(&uri);
        }
    }

    pub(super) fn clear(&self, uri: &str) {
        self.records.remove(uri);
    }

    /// The report for `uri`: a fresh one (published since the last sync, at
    /// or above `min_version`), or, with `allow_superseded`, the one the
    /// last sync superseded, flagged `superseded: true`.
    pub(super) fn report(
        &self,
        uri: &str,
        min_version: Option<i64>,
        allow_superseded: bool,
    ) -> Option<Value> {
        // Memo releases its lock after cloning only the Arc: materializing a
        // large report never holds up other document publications or reads.
        let record = self.records.get(uri)?.value;
        let superseded = record.superseded.load(Ordering::Relaxed);
        if superseded && !allow_superseded {
            return None;
        }
        if let Some(min_version) = min_version
            && !superseded
        {
            // A record tagged with an older version belongs to a superseded
            // sync and must not be returned as fresh. A record WITHOUT a
            // version (typescript-language-server never sends one) is
            // accepted as current: every document sync marks the record it
            // held, so an unmarked one was published after that sync.
            if let Some(version) = record.params.get("version").and_then(Value::as_i64)
                && version < min_version
            {
                return None;
            }
        }
        Some(json!({
            "kind": "full",
            "items": record.params.get("diagnostics").cloned().unwrap_or_else(|| json!([])),
            "version": record.params.get("version").cloned().unwrap_or(Value::Null),
            "source": "push",
            "superseded": superseded,
        }))
    }

    /// Waits until every one of `documents` (`uri`, minimum version) has a
    /// fresh report, or `until`; `true` when every one has.
    pub(super) async fn wait_for_all(
        &self,
        documents: &[(String, Option<i64>)],
        until: Instant,
    ) -> bool {
        let all_fresh = || {
            documents
                .iter()
                .all(|(uri, min_version)| self.report(uri, *min_version, false).is_some())
        };
        loop {
            if all_fresh() {
                return true;
            }
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if all_fresh() {
                return true;
            }
            if timeout_at(until, notified).await.is_err() {
                return all_fresh();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test(start_paused = true)]
    async fn an_unretainable_report_is_unavailable_instead_of_clean_or_stale() {
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
                .wait_for_all(&[(uri.to_owned(), None)], Instant::now())
                .await
                && store.report(uri, None, true).is_none(),
            "no invented empty report"
        );
        assert_eq!(BUDGET.used(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn push_diagnostics_ignores_notifications_for_other_documents() {
        let store = PushDiagnosticsStore::new();
        let waiter_store = Arc::clone(&store);
        let waiter = tokio::spawn(async move {
            let until = Instant::now() + std::time::Duration::from_secs(1);
            waiter_store
                .wait_for_all(&[("file:///workspace/a.ts".to_owned(), Some(3))], until)
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
    }
}

#[cfg(test)]
mod supersede_tests {
    use super::*;

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
    #[tokio::test(start_paused = true)]
    async fn a_superseded_report_stands_in_only_after_the_wait() {
        let store = PushDiagnosticsStore::new();
        store.record(json!({
            "uri": "file:///w/a.ts",
            "diagnostics": [{ "message": "before" }]
        }));
        store.supersede("file:///w/a.ts");
        assert!(store.report("file:///w/a.ts", None, false).is_none());
        let until = Instant::now() + std::time::Duration::from_millis(50);
        assert!(
            !store
                .wait_for_all(&[("file:///w/a.ts".to_owned(), None)], until)
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
    }
}
