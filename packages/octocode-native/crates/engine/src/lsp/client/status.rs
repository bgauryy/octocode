//! Readiness, liveness, capability, and busy-state queries.

use super::{LspLease, NativeLspClient};
use crate::error::Result;
use crate::lsp::transport::{Readiness, STARTUP_SETTLE_MS};
use serde_json::Value;
use std::sync::{Arc, atomic::Ordering};
use tokio::time::Duration;

impl NativeLspClient {
    /// Wait for the server to finish any post-`initialized` indexing, returning
    /// a readiness descriptor so JS can tell a confirmed-idle server apart from
    /// one that never reported progress or is still busy. The returned string
    /// is one of `"progressIdle"`, `"settledWithoutProgress"`, or `"timeout"`.
    pub(crate) async fn wait_for_ready(&self, timeout_ms: Option<u32>) -> Result<String> {
        crate::lsp::diagnostics::Phase::Readiness
            .measure(async {
                let timeout_ms = u64::from(timeout_ms.unwrap_or(45_000));
                let readiness = self
                    .inner
                    .progress
                    .wait_until_idle(timeout_ms, self.settle_ms(self.startup_settle_ms()))
                    .await
                    .as_str()
                    .to_owned();
                {
                    let mut slot = self
                        .inner
                        .readiness
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *slot = Some(readiness.clone());
                }
                Ok(readiness)
            })
            .await
    }

    /// `false` if the client was never started/already stopped, or if its
    /// connection's read loop has observed the server process close (crash).
    /// Lets the JS client pool evict a stale pooled entry at the next
    /// `acquire()` instead of returning a client whose requests will just
    /// fail until the idle timer eventually reaps it.
    pub async fn is_alive(&self) -> bool {
        match self.inner.connection.lock().await.as_ref() {
            Some(connection) => connection.is_alive(),
            None => false,
        }
    }

    /// See [`crate::lsp::config::serves_requests_concurrently`].
    pub fn serves_requests_concurrently(&self) -> bool {
        crate::lsp::config::serves_requests_concurrently(&self.inner.config)
    }

    /// See [`crate::lsp::config::workspace_symbol_limit`].
    pub fn workspace_symbol_limit(&self) -> Option<usize> {
        crate::lsp::config::workspace_symbol_limit(&self.inner.config)
    }

    pub fn has_capability(&self, capability: String) -> bool {
        let capabilities = self
            .inner
            .capabilities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        capabilities
            .as_ref()
            .map(|value| capability_supported(value, &capability))
            .unwrap_or(false)
    }

    pub fn readiness(&self) -> Option<String> {
        self.inner
            .readiness
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// How long startup waits for a first `$/progress`: none for a server
    /// that announces its project load only once a document is open (the
    /// TypeScript family: tsserver starts loading after `didOpen`), where the
    /// open wait catches it; [`STARTUP_SETTLE_MS`] otherwise.
    fn startup_settle_ms(&self) -> u64 {
        if crate::lsp::family::is_typescript_family(self.inner.config.language_id.as_deref()) {
            0
        } else {
            STARTUP_SETTLE_MS
        }
    }

    /// `settle` bounded by [`LspClientOptions::settle_cap`].
    pub(super) fn settle(&self, settle: Duration) -> Duration {
        self.inner
            .options
            .settle_cap
            .map_or(settle, |cap| settle.min(cap))
    }

    /// [`Self::settle`] in milliseconds.
    pub(super) fn settle_ms(&self, settle_ms: u64) -> u64 {
        u64::try_from(self.settle(Duration::from_millis(settle_ms)).as_millis())
            .unwrap_or(settle_ms)
    }

    pub(super) fn readiness_is(&self, readiness: Readiness) -> bool {
        self.inner
            .readiness
            .lock()
            .is_ok_and(|slot| slot.as_deref() == Some(readiness.as_str()))
    }

    pub(crate) fn language_id(&self) -> Option<&str> {
        self.inner.config.language_id.as_deref()
    }

    /// A pooled client whose last readiness wait ended in `timeout` waits
    /// again, up to `timeout_ms`, before it is handed out: the server kept
    /// indexing since, so the next call can see it finish instead of failing
    /// on the stored `timeout`. Any other readiness returns at once. The
    /// re-wait never pays the silent-server settle again: a `timeout` means
    /// the server already reported progress. The wait holds a lease, so idle
    /// expiry cannot stop the server under it.
    pub(crate) async fn resume_timed_out_readiness(&self, timeout_ms: u32) {
        if !self.readiness_is(Readiness::Timeout) {
            return;
        }
        let _activity = self.lease();
        let _ = self.wait_for_ready(Some(timeout_ms)).await;
    }

    /// Mark this client busy for as long as the returned guard lives. Hold
    /// one across a whole multi-request operation (document syncs, readiness
    /// waits, and the gaps between requests) so the pool's idle timer and LRU
    /// eviction never stop the server mid-operation. Requests, syncs, and
    /// diagnostic waits take their own lease internally.
    pub fn lease(&self) -> LspLease {
        LspLease::new(&self.inner.activity)
    }

    /// `true` when `other` is a clone of this client (same server process
    /// state), not merely one with an equal config.
    pub(crate) fn same_client(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// `true` while any request, sync, diagnostic wait, or [`LspLease`] is
    /// outstanding on this client (or a clone of it).
    pub(crate) fn is_busy(&self) -> bool {
        self.inner.activity.load(Ordering::Acquire) > 0
    }

    /// The server tree's resident bytes at the last watchdog sample.
    pub(crate) fn last_resident_bytes(&self) -> u64 {
        self.inner.resident_bytes.load(Ordering::Relaxed)
    }
}

fn capability_supported(capabilities: &Value, capability: &str) -> bool {
    capabilities
        .get(capability)
        .map(capability_value_supported)
        .unwrap_or(false)
}

fn capability_value_supported(value: &Value) -> bool {
    match value {
        Value::Bool(enabled) => *enabled,
        Value::Null => false,
        Value::Object(_) => true,
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}
