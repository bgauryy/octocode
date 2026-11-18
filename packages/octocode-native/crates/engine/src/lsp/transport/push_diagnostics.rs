//! Push-diagnostics cache for a JSON-RPC LSP connection.
//!
//! Records complete `textDocument/publishDiagnostics` notifications the server
//! pushes unprompted against the process's shared memory budget, and lets callers
//! wait for a report at (or above) a requested document version. The transport
//! admits each whole notification before this store sees it; pagination belongs
//! to the caller, so this store must never discard a report's suffix.

use crate::lsp::clock::LspClock;
use octocode_io::memo::{Memo, MemoryBudget};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use tokio::time::Instant;

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
    /// fresh report, or `until` on `clock`; `true` when every one has.
    pub(super) async fn wait_for_all(
        &self,
        documents: &[(String, Option<i64>)],
        until: Instant,
        clock: &LspClock,
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
            if clock.timeout_at(until, notified).await.is_err() {
                return all_fresh();
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod supersede_tests;
