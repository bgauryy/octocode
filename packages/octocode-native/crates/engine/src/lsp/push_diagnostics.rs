//! Push-diagnostics cache for a JSON-RPC LSP connection.
//!
//! Records `textDocument/publishDiagnostics` notifications the server pushes
//! unprompted, bounds their retained document/entry/byte counts, and lets
//! callers wait for a report at (or above) a requested document version.
//! Extracted verbatim from `json_rpc.rs`; behaviour is unchanged. The size
//! bounds live in the parent module and are imported via `super::` so both the
//! store and the parent's tests share a single source of truth.

use super::json_rpc::{
    MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT, MAX_PUSH_DIAGNOSTIC_DOCUMENTS,
    MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT,
};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Notify;
use tokio::time::{Duration, Instant, timeout};

#[derive(Clone)]
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

    pub(super) fn record(&self, params: &Value) {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return;
        };
        let mut params = params.clone();
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
        state.order.retain(|existing| existing != uri);
        if !state.records.contains_key(uri)
            && state.records.len() >= MAX_PUSH_DIAGNOSTIC_DOCUMENTS
            && let Some(oldest) = state.order.pop_front()
        {
            state.records.remove(&oldest);
        }
        state.order.push_back(uri.to_owned());
        state
            .records
            .insert(uri.to_owned(), PushDiagnosticsRecord { params, truncated });
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
