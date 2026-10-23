//! Document synchronization: the `didOpen` → `didChange` → `didClose`
//! lifecycle, and keeping open documents in line with their files.

use super::open_documents::{DiskCheck, content_digest};
use super::{NativeLspClient, OPEN_READY_TIMEOUT_MS, OPEN_SETTLE_MS};
use crate::error::{Error, Result};
use crate::lsp::transport::{JsonRpcConnection, Readiness};
use crate::lsp::uri::path_to_uri;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::Ordering;

/// Largest post-open wait any caller may ask for.
const MAX_OPEN_READY_TIMEOUT_MS: u32 = 60_000;

/// `didClose` for `uri`, dropping its push diagnostics first.
async fn notify_close(connection: &JsonRpcConnection, uri: &str) -> Result<()> {
    connection.clear_push_diagnostics(uri);
    connection
        .notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await
}

impl NativeLspClient {
    /// Sync a document's in-memory content to the server, honoring the LSP
    /// document lifecycle: the FIRST sync of a URI sends `textDocument/didOpen`
    /// (version 1); a later sync of changed content sends
    /// `textDocument/didChange` with an incremented version and a
    /// full-document content change, and a sync of the content the server
    /// already holds sends nothing. Re-sending
    /// `didOpen` is ignored or rejected by many servers and can make
    /// changed content resolve against the stale original.
    pub async fn open_document(&self, file_path: String, content: &str) -> Result<()> {
        self.sync_document(&file_path, content).await.map(|_| ())
    }

    /// [`Self::open_document`], then — only when this sync was the document's
    /// first `didOpen` — wait for any project load the open triggered to go
    /// idle before returning. Servers like `typescript-language-server` start
    /// loading the project only after a `didOpen` (`$/progress begin` arrives
    /// ~100 ms later); querying immediately races that load and returns
    /// incomplete references. `settle_ms` bounds how long to wait for such a
    /// wave to start (default [`OPEN_SETTLE_MS`]) and `timeout_ms` bounds the
    /// whole wait (default [`OPEN_READY_TIMEOUT_MS`], capped at 60 s).
    ///
    /// The settle is not stacked on a startup readiness wait that already
    /// confirmed the project loaded (`progressIdle`): that server announced
    /// and finished its project load before this open, so only progress
    /// already under way when the open was written is drained.
    ///
    /// Returns the readiness string (`progressIdle`, `settledWithoutProgress`, or
    /// `timeout`) for a first open, and `None` for a re-sync of an already open
    /// document, which does not wait.
    pub async fn open_document_and_wait(
        &self,
        file_path: String,
        content: &str,
        settle_ms: Option<u32>,
        timeout_ms: Option<u32>,
    ) -> Result<Option<String>> {
        let _activity = self.lease();
        let observed = self.inner.progress.subscribe();
        let version = self.sync_document(&file_path, content).await?;
        if version != Some(1) {
            return Ok(None);
        }
        let settle_ms = if self.readiness_is(Readiness::ProgressIdle) {
            0
        } else {
            self.settle_ms(u64::from(settle_ms.unwrap_or(OPEN_SETTLE_MS)))
        };
        let timeout_ms = u64::from(
            timeout_ms
                .unwrap_or(OPEN_READY_TIMEOUT_MS)
                .min(MAX_OPEN_READY_TIMEOUT_MS),
        );
        let readiness = self
            .inner
            .progress
            .wait_until_idle_after(observed, settle_ms, timeout_ms)
            .await;
        Ok(Some(readiness.as_str().to_owned()))
    }

    /// Sync `content` for `file_path` and return the version that was sent
    /// (`1` means a fresh `didOpen`), or `None` when the server already holds
    /// `content`.
    async fn sync_document(&self, file_path: &str, content: &str) -> Result<Option<i32>> {
        let file_path = file_path.to_owned();
        let uri = path_to_uri(&file_path)?;
        let _activity = self.lease();
        // Held from version reservation until the notification is written:
        // two concurrent syncs of one document must reach the server in
        // version order (never `didChange v2` before `didOpen v1`).
        let _ordered = self.inner.sync_lock.lock().await;

        // Acquire the connection FIRST: if the client isn't started this fails
        // without mutating `open_docs`, so a doc is never marked open when its
        // didOpen/didChange was never actually sent.
        let connection = self.connection_handle().await?;
        // The server resolves this document's project setup now (at its
        // first `didOpen`): the baseline for noticing a later change.
        self.observe_project_config(Path::new(&file_path));
        // Content the server already holds is not sent again: a resync would
        // make the server re-analyze an unchanged document (and drop its
        // diagnostics) on every request.
        let digest = content_digest(content);
        if self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .unchanged(&uri, digest)
            .is_some()
        {
            return Ok(None);
        }
        // A content sync invalidates any push diagnostics for the prior
        // document version. The next diagnostic read waits for a fresh publish.
        connection.clear_push_diagnostics(&uri);

        // Decide didOpen-vs-didChange and reserve the version under the lock,
        // then release it before awaiting the notify (never hold a std mutex
        // across an await). `reserve` also enforces the LRU cap, handing back
        // the URI of any evicted least-recently-synced document so we can close
        // it out below and keep the open-document set bounded.
        let (next_version, evicted) = {
            let mut open_docs = self
                .inner
                .open_docs
                .lock()
                .map_err(|_| Error::new("open_docs lock poisoned"))?;
            open_docs.reserve(&uri)
        };

        let notification = if next_version == 1 {
            let language_id = crate::lsp::config::detect_language_id(&file_path)
                .or_else(|| self.inner.config.language_id.clone())
                .unwrap_or_else(|| "plaintext".to_owned());
            let params = json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": next_version,
                    "text": content
                }
            });
            connection.notify("textDocument/didOpen", params).await
        } else {
            let params = json!({
                "textDocument": { "uri": uri, "version": next_version },
                "contentChanges": [{ "text": content }]
            });
            connection.notify("textDocument/didChange", params).await
        };
        {
            let mut open_docs = self
                .inner
                .open_docs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &notification {
                Ok(()) => open_docs.record(&uri, next_version, &file_path, digest),
                Err(_) => open_docs.rollback(&uri, next_version),
            }
        }
        if notification.is_ok() && next_version > 1 {
            self.inner.content_epoch.fetch_add(1, Ordering::AcqRel);
        }
        // Close the document the cap evicted (if any) so both our bookkeeping
        // and the server's document set stay bounded. Best-effort: a stopped or
        // wedged connection makes this moot, and it must not mask the primary
        // notification result.
        if let Some(evicted_uri) = evicted {
            let _ = notify_close(&connection, &evicted_uri).await;
        }
        notification.map(|()| Some(next_version))
    }

    fn observe_project_config(&self, path: &Path) {
        let mut project_config = self
            .inner
            .project_config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        project_config.observe(path);
    }

    /// Whether a project config file the server reads (`compile_commands.json`,
    /// `compile_flags.txt`, `.clangd`) was created, changed or deleted in a
    /// directory it resolved a document through since it first looked there.
    /// The server keeps the setup it saw until it is replaced, so the pool
    /// replaces a changed-under client once it is idle. Always `false` for a
    /// server that re-reads its own configuration.
    pub(crate) async fn project_config_changed(&self) -> bool {
        let watch = {
            let watch = self
                .inner
                .project_config
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !watch.is_active() {
                return false;
            }
            watch.clone()
        };
        tokio::task::spawn_blocking(move || watch.changed())
            .await
            .unwrap_or(false)
    }

    /// Bring every open document back in line with its file before a
    /// request. The server answers from an open document's synced text, not
    /// from disk, so a file edited after it was opened would otherwise keep
    /// answering with its old lines whichever document the request names. A
    /// document whose settled stamp still matches is not read; a changed one
    /// is re-synced (`didChange`); one whose file is gone or unreadable is
    /// closed, so the server reads the disk again.
    pub(super) async fn sync_open_documents(&self) -> Result<()> {
        let synced = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .synced();
        if synced.is_empty() {
            return Ok(());
        }
        let _activity = self.lease();
        let checks = tokio::task::spawn_blocking(move || {
            synced
                .into_iter()
                .filter_map(|(uri, synced)| DiskCheck::of(uri, synced))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|error| Error::new(format!("open document check failed: {error}")))?;
        for check in checks {
            let (uri, digest, stamp) = match check {
                DiskCheck::Same { uri, digest, stamp } => (uri, digest, stamp),
                DiskCheck::Changed {
                    uri,
                    path,
                    content,
                    stamp,
                } => {
                    self.sync_document(&path, &content).await?;
                    (uri, content_digest(&content), stamp)
                }
                DiskCheck::Gone { uri } => {
                    self.close_document(&uri).await?;
                    continue;
                }
            };
            self.inner
                .open_docs
                .lock()
                .map_err(|_| Error::new("open_docs lock poisoned"))?
                .stamp(&uri, digest, stamp);
        }
        Ok(())
    }

    /// Close the open document `uri` (its file is gone or unreadable), so
    /// the server answers from its own view of the disk again.
    async fn close_document(&self, uri: &str) -> Result<()> {
        let _ordered = self.inner.sync_lock.lock().await;
        let connection = self.connection_handle().await?;
        let was_open = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .forget(uri);
        if !was_open {
            return Ok(());
        }
        let closed = notify_close(&connection, uri).await;
        self.inner.content_epoch.fetch_add(1, Ordering::AcqRel);
        closed
    }
}
