//! The client surface the pool manages, so the pool logic is tested against
//! a fake client while production pools [`NativeLspClient`]s.

use super::readiness_timeout;
use crate::lsp::client::NativeLspClient;
use std::future::Future;
use std::pin::Pin;

pub(super) type ClientFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub(super) trait PoolClient: Clone + Send + Sync + 'static {
    fn alive(&self) -> ClientFuture<bool>;
    /// Authoritative "in use" signal. The pool never stops a busy client: the
    /// idle timer re-arms while it is busy and LRU eviction skips it (allowing a
    /// temporary overflow instead). It MUST therefore stay `true` for the whole
    /// of any operation holding the client — in-flight requests AND any lease
    /// held across document syncs, readiness waits, and the gaps between the
    /// requests of one multi-request walk — not only while a single request is
    /// outstanding.
    fn busy(&self) -> bool;
    /// Resident memory of the server tree at its last sample; `0` when
    /// unsampled (no memory cap configured), which counts against nothing.
    fn resident_bytes(&self) -> u64;
    fn stop(&self) -> ClientFuture<()>;
    /// Same underlying client (a clone of it), not merely an equal config.
    fn same(&self, other: &Self) -> bool;
    /// Runs on a live pooled client before it is handed out again. A client
    /// whose readiness wait timed out waits again here, so a stored
    /// `timeout` never fails every later call while the server indexes.
    /// Concurrent acquires of the key share this one wait.
    fn resume(&self) -> ClientFuture<()>;
    /// A project config file the server reads once and never re-reads for a
    /// document it holds changed under it (see `lsp::project_config`): the
    /// pool replaces the server when it is idle.
    fn project_config_changed(&self) -> ClientFuture<bool>;
}

impl PoolClient for NativeLspClient {
    fn alive(&self) -> ClientFuture<bool> {
        let client = self.clone();
        Box::pin(async move { client.is_alive().await })
    }

    fn busy(&self) -> bool {
        // Counts requests, syncs, diagnostic waits, and caller leases.
        self.is_busy()
    }

    fn resident_bytes(&self) -> u64 {
        self.last_resident_bytes()
    }

    fn stop(&self) -> ClientFuture<()> {
        let client = self.clone();
        Box::pin(async move {
            let _ = client.stop().await;
        })
    }

    fn same(&self, other: &Self) -> bool {
        self.same_client(other)
    }

    fn resume(&self) -> ClientFuture<()> {
        let client = self.clone();
        Box::pin(async move {
            if let Some(timeout_ms) = readiness_timeout(client.language_id()) {
                client.resume_timed_out_readiness(timeout_ms).await;
            }
        })
    }

    fn project_config_changed(&self) -> ClientFuture<bool> {
        let client = self.clone();
        Box::pin(async move { client.project_config_changed().await })
    }
}
