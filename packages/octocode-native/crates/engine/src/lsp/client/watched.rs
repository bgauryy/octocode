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

/// LSP `FileChangeType`.
const CREATED: u8 = 1;
const CHANGED: u8 = 2;
const DELETED: u8 = 3;

impl NativeLspClient {
    /// Whether the server registered for `workspace/didChangeWatchedFiles`.
    pub async fn watches_files(&self) -> bool {
        match self.inner.connection.lock().await.as_ref() {
            Some(connection) => connection.watches_files(),
            None => false,
        }
    }

    /// Replace the remembered scope snapshot (relative path → stamp) with
    /// `current` and return what changed: `(relative path, change type)`
    /// for every created, changed or deleted file. `None` when there was no
    /// snapshot to compare with (the first request of this process).
    #[must_use]
    pub fn watched_files_diff(&self, current: &BTreeMap<String, String>) -> Option<Vec<(String, u8)>> {
        let mut slot = self
            .inner
            .watched_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = slot.replace(current.clone())?;
        let mut changes = Vec::new();
        for (path, stamp) in current {
            match previous.get(path) {
                None => changes.push((path.clone(), CREATED)),
                Some(before) if before != stamp => changes.push((path.clone(), CHANGED)),
                Some(_) => {}
            }
        }
        for path in previous.keys() {
            if !current.contains_key(path) {
                changes.push((path.clone(), DELETED));
            }
        }
        Some(changes)
    }

    /// Send `workspace/didChangeWatchedFiles` for `changes` (`(uri, change
    /// type)`), bumping the content epoch so no cached answer predates it.
    pub async fn notify_watched_files(&self, changes: Vec<(String, u8)>) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let connection = self.connection_handle().await?;
        let changes = changes
            .into_iter()
            .map(|(uri, kind)| json!({ "uri": uri, "type": kind }))
            .collect::<Vec<Value>>();
        self.inner
            .content_epoch
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        connection
            .notify(
                "workspace/didChangeWatchedFiles",
                json!({ "changes": changes }),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::LspLanguageServerConfig;

    fn client() -> NativeLspClient {
        NativeLspClient::new(LspLanguageServerConfig {
            command: "server".into(),
            args: None,
            workspace_root: "/w".into(),
            language_id: Some("go".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: Default::default(),
        })
    }

    #[test]
    fn the_first_snapshot_owes_no_events_and_later_ones_name_each_change() {
        let client = client();
        let first = BTreeMap::from([
            ("a.go".to_owned(), "1".to_owned()),
            ("b.go".to_owned(), "1".to_owned()),
        ]);
        assert!(client.watched_files_diff(&first).is_none());
        let second = BTreeMap::from([
            ("a.go".to_owned(), "2".to_owned()),
            ("c.go".to_owned(), "1".to_owned()),
        ]);
        let mut changes = client.watched_files_diff(&second).expect("a snapshot to compare with");
        changes.sort();
        assert_eq!(
            changes,
            vec![
                ("a.go".to_owned(), CHANGED),
                ("b.go".to_owned(), DELETED),
                ("c.go".to_owned(), CREATED),
            ]
        );
        assert_eq!(client.watched_files_diff(&second), Some(Vec::new()));
    }
}
