//! File events for a server that registered for
//! `workspace/didChangeWatchedFiles`. The client runs no watcher: each
//! request hands it the scope's files with their stamps, and the server is
//! told what changed since the last request, so a file no request opened
//! (a new importer, a deleted module, regenerated output) still reaches
//! the server's index. A fresh process indexes the disk itself and starts
//! with no snapshot.

use super::NativeLspClient;
use crate::error::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// LSP `FileChangeType`.
const CREATED: u8 = 1;
const CHANGED: u8 = 2;
const DELETED: u8 = 3;

/// A source snapshot and the events that bring the server to that snapshot.
/// Build this from freshly captured files inside `sync_watched_files`' callback;
/// callers may perform capture and content comparison on their blocking worker.
pub struct WatchedFileUpdate {
    current: BTreeMap<String, String>,
    changes: Vec<Value>,
}

impl WatchedFileUpdate {
    pub fn new(
        root: &Path,
        previous: Option<&BTreeMap<String, String>>,
        current: BTreeMap<String, String>,
        unchanged: impl Fn(&str, &str, &str) -> bool,
    ) -> Result<Self> {
        let changes = previous
            .map(|previous| changed_files(previous, &current, unchanged))
            .unwrap_or_default()
            .into_iter()
            .map(|(relative, kind)| {
                crate::lsp::uri::path_to_uri(&root.join(relative).to_string_lossy())
                    .map(|uri| json!({ "uri": uri, "type": kind }))
            })
            .collect::<Result<Vec<Value>>>()?;
        Ok(Self { current, changes })
    }
}

impl NativeLspClient {
    /// Whether the server registered for `workspace/didChangeWatchedFiles`.
    pub async fn watches_files(&self) -> bool {
        match self.inner.connection.lock().await.as_ref() {
            Some(connection) => connection.watches_files(),
            None => false,
        }
    }

    /// Whether a scope snapshot is remembered for this server process.
    #[must_use]
    pub fn has_watched_snapshot(&self) -> bool {
        self.inner
            .watched_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Capture current files, compare, notify and commit under one lock. The
    /// callback must capture its snapshot after being called, rather than pass
    /// a snapshot prepared while another request could still advance the server.
    /// Failed capture or notification leaves the previous proof intact. The
    /// first snapshot records the disk the fresh server indexed and owes no events.
    /// Failure or cancellation before that baseline retires the unproved session.
    pub async fn sync_watched_files<E, F, Fut>(&self, prepare: F) -> std::result::Result<(), E>
    where
        E: From<crate::error::Error>,
        F: FnOnce(Option<BTreeMap<String, String>>) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<WatchedFileUpdate, E>>,
    {
        let _ordered = self.inner.sync_lock.lock().await;
        let connection = self.connection_handle().await.map_err(E::from)?;
        let previous = self
            .inner
            .watched_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        // Without an initial proof, a failed or dropped capture leaves no way
        // to discover files deleted before retry. A fresh server must reindex.
        let mut initial = InitialSnapshot(previous.is_none().then_some(connection.as_ref()));
        let update = prepare(previous).await?;
        if !update.changes.is_empty() {
            connection.supersede_all_push_diagnostics();
            connection
                .notify(
                    "workspace/didChangeWatchedFiles",
                    json!({ "changes": update.changes }),
                )
                .await
                .map_err(E::from)?;
        }
        *self
            .inner
            .watched_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(update.current);
        initial.0 = None;
        Ok(())
    }
}

struct InitialSnapshot<'a>(Option<&'a crate::lsp::transport::JsonRpcConnection>);

impl Drop for InitialSnapshot<'_> {
    fn drop(&mut self) {
        if let Some(connection) = self.0 {
            connection.retire(
                "Initial watched-file snapshot did not complete; retry with a fresh LSP session",
            );
        }
    }
}

fn changed_files(
    previous: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
    unchanged: impl Fn(&str, &str, &str) -> bool,
) -> Vec<(String, u8)> {
    let mut changes = Vec::new();
    for (path, stamp) in current {
        match previous.get(path) {
            None => changes.push((path.clone(), CREATED)),
            Some(before) if before != stamp && !unchanged(path, before, stamp) => {
                changes.push((path.clone(), CHANGED));
            }
            Some(_) => {}
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            changes.push((path.clone(), DELETED));
        }
    }
    changes
}

#[cfg(test)]
mod tests;
