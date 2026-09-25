//! Push-diagnostics cache for a JSON-RPC LSP connection.
//!
//! Records `textDocument/publishDiagnostics` notifications the server pushes
//! unprompted, bounds their retained document/entry/byte counts, and lets
//! callers wait for a report at (or above) a requested document version.

use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Notify;
use tokio::time::{Duration, Instant, timeout};

/// Documents retained at once; the least recently published is evicted first.
const MAX_PUSH_DIAGNOSTIC_DOCUMENTS: usize = 256;
const MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT: usize = 2_000;
const MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT: usize = 256 * 1024;

struct PushDiagnosticsRecord {
    params: Value,
    truncated: bool,
}

#[derive(Default)]
struct PushDiagnosticsState {
    records: HashMap<String, PushDiagnosticsRecord>,
    order: VecDeque<String>,
}

#[derive(Default)]
pub(super) struct PushDiagnosticsStore {
    state: StdMutex<PushDiagnosticsState>,
    changed: Notify,
}

impl PushDiagnosticsStore {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn record(&self, mut params: Value) {
        let Some(uri) = params.get("uri").and_then(Value::as_str).map(str::to_owned) else {
            return;
        };
        let mut truncated = false;
        if let Some(items) = params.get_mut("diagnostics").and_then(Value::as_array_mut) {
            if items.len() > MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT {
                items.truncate(MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT);
                truncated = true;
            }
            let mut retained_bytes = 2usize; // JSON array brackets
            let mut retained_len = items.len();
            for (index, item) in items.iter().enumerate() {
                let item_bytes = serde_json::to_vec(item)
                    .map(|encoded| encoded.len())
                    .unwrap_or(MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT + 1);
                let next_bytes = retained_bytes
                    .saturating_add(usize::from(index > 0))
                    .saturating_add(item_bytes);
                if next_bytes > MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT {
                    retained_len = index;
                    truncated = true;
                    break;
                }
                retained_bytes = next_bytes;
            }
            items.truncate(retained_len);
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.order.retain(|existing| *existing != uri);
        if !state.records.contains_key(&uri)
            && state.records.len() >= MAX_PUSH_DIAGNOSTIC_DOCUMENTS
            && let Some(oldest) = state.order.pop_front()
        {
            state.records.remove(&oldest);
        }
        state.order.push_back(uri.clone());
        state
            .records
            .insert(uri, PushDiagnosticsRecord { params, truncated });
        drop(state);
        self.changed.notify_waiters();
    }

    pub(super) fn clear(&self, uri: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.records.remove(uri);
            state.order.retain(|existing| existing != uri);
        }
    }

    pub(super) fn report(&self, uri: &str, min_version: Option<i64>) -> Option<Value> {
        let state = self.state.lock().ok()?;
        let record = state.records.get(uri)?;
        if let Some(min_version) = min_version {
            // A record tagged with an older version belongs to a superseded
            // sync and must not be returned. A record WITHOUT a version (e.g.
            // typescript-language-server never sends one) is accepted as
            // current: every document sync clears the cached record first, so
            // any record present afterwards was published after that sync.
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
            "truncated": record.truncated,
        }))
    }

    pub(super) async fn wait_for(
        &self,
        uri: &str,
        timeout_ms: u32,
        min_version: Option<i64>,
    ) -> Option<Value> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        loop {
            if let Some(report) = self.report(uri, min_version) {
                return Some(report);
            }
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(report) = self.report(uri, min_version) {
                return Some(report);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || timeout(remaining, notified).await.is_err() {
                return None;
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

        assert!(store.report("file:///workspace/a.ts", Some(3)).is_none());
        assert_eq!(
            store
                .report("file:///workspace/a.ts", Some(2))
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
                .report("file:///workspace/a.ts", Some(3))
                .expect("versionless report accepted for a min version")["items"][0]["message"],
            "no-version"
        );
        assert_eq!(
            store
                .report("file:///workspace/a.ts", None)
                .expect("versionless report readable without a min")["items"][0]["message"],
            "no-version"
        );
    }

    #[test]
    fn push_diagnostics_bounds_retained_bytes() {
        let store = PushDiagnosticsStore::new();
        store.record(json!({
            "uri": "file:///workspace/a.ts",
            "version": 3,
            "diagnostics": [{
                "message": "x".repeat(MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT + 1)
            }]
        }));

        let report = store
            .report("file:///workspace/a.ts", Some(3))
            .expect("bounded report");
        assert_eq!(report["truncated"], true);
        assert_eq!(report["items"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn push_diagnostics_bounds_documents_and_entries() {
        let store = PushDiagnosticsStore::new();
        for index in 0..=MAX_PUSH_DIAGNOSTIC_DOCUMENTS {
            store.record(json!({
                "uri": format!("file:///w/{index}.ts"),
                "diagnostics": vec![json!({"message": "m"}); MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT + 1]
            }));
        }
        assert!(
            store.report("file:///w/0.ts", None).is_none(),
            "oldest evicted"
        );
        let newest = store
            .report(
                &format!("file:///w/{MAX_PUSH_DIAGNOSTIC_DOCUMENTS}.ts"),
                None,
            )
            .expect("newest kept");
        assert_eq!(newest["truncated"], true);
        assert_eq!(
            newest["items"].as_array().map(Vec::len),
            Some(MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn push_diagnostics_ignores_notifications_for_other_documents() {
        let store = PushDiagnosticsStore::new();
        let waiter_store = Arc::clone(&store);
        let waiter = tokio::spawn(async move {
            waiter_store
                .wait_for("file:///workspace/a.ts", 1_000, Some(3))
                .await
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
