//! The immutable, content-addressed capture storage a URL capture saves to
//! and its read lead reads back.
use super::{Captured, Selection, SourceReader, capture_root};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::tools::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

/// The capture file inside its content-addressed directory.
const CAPTURE_FILE: &str = "source.txt";

/// The runtime's URL capture storage.
pub(in crate::tools::source_capture) struct Store<'a> {
    root: PathBuf,
    ttl: Duration,
    paths: &'a PathPolicy,
}

impl<'a> Store<'a> {
    /// The storage, when a capture can be saved and read back here.
    pub(in crate::tools::source_capture) fn open(
        dispatcher: &'a dyn SourceReader,
    ) -> Result<Self, ClassificationError> {
        if !crate::config::is_persistent_storage_enabled(&dispatcher.config().resolved) {
            return Err(ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "Source captures need persistent storage to save the exact evidence; storage.mode is memory.",
                "Use persistent storage for unread source captures.",
            ));
        }
        let reader = ToolId::LocalFetch.as_str();
        if !dispatcher.availability().is_available(reader) {
            return Err(ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                format!(
                    "Source capture storage requires {reader}, which is not available in this runtime."
                ),
                "Enable localFetch for source capture storage.",
            ));
        }
        Self::at(
            capture_root(dispatcher.home()),
            // The clone cache retention: each capture publishes it as
            // `expiresAt`; the daily cache sweep removes captures older.
            dispatcher.limits().clone.ttl,
            dispatcher.paths(),
        )
    }

    pub(super) fn at(
        root: PathBuf,
        ttl: Duration,
        paths: &'a PathPolicy,
    ) -> Result<Self, ClassificationError> {
        // The policy judges the directory itself, so it must exist: an
        // allowed root's missing parents are not inside that root.
        let outside = |_| {
            ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "The capture directory is outside the local path policy.",
                "Supply the page text as content.",
            )
        };
        octocode_io::atomic::create_private_dir_all(&root).map_err(|_| {
            ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "The capture directory could not be created.",
                "Check that the Octocode home is writable, or supply the text as content.",
            )
        })?;
        paths.validate(&root).map_err(outside)?;
        Ok(Self { root, ttl, paths })
    }

    /// Save the selection immutably and build its state, provenance and
    /// read lead.
    pub(super) fn save(&self, selection: Selection) -> Result<Captured, ClassificationError> {
        let bytes = selection.text.as_bytes();
        let saved = self.save_bytes(bytes, CAPTURE_FILE)?;
        let snapshot = saved["snapshot"].as_str().unwrap_or_default();
        let file = saved["path"].as_str().unwrap_or_default();
        let lines = selection.text.lines().count();
        let url = selection.url.as_str();
        let read = crate::response::result::Continuation::continued(
            ToolId::LocalFetch,
            json!({
                "path": file,
                "snapshot": snapshot,
                "ranges": [{"startLine":1,"endLine":lines}],
            }),
        )
        .build();
        Ok(Captured {
            state: json!({
                "source": { "url": url, "section": selection.section },
                "text": selection.text,
            }),
            read,
            source: Some(
                json!({ "url": url, "snapshot": snapshot, "expiresAt": saved["expiresAt"] }),
            ),
        })
    }

    /// Save exact source bytes without interpreting them as text. Images
    /// share the existing immutable capture storage, authorization and TTL.
    pub(in crate::tools::source_capture) fn save_bytes(
        &self,
        bytes: &[u8],
        name: &str,
    ) -> Result<serde_json::Value, ClassificationError> {
        let snapshot = octocode_engine::digest::sha256(bytes);
        let directory = self.root.join(&snapshot);
        let file = directory.join(name);
        let unsaved = |_| {
            ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "The captured source could not be saved; nothing was judged.",
                "Check that the Octocode home is writable.",
            )
        };
        octocode_io::atomic::create_private_dir_all(&directory).map_err(unsaved)?;
        octocode_io::atomic::write_atomic(&file, bytes, false).map_err(unsaved)?;
        let readable = self.paths.validate_read(&file).map_err(|_| {
            ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "The saved capture is not readable under the local path policy.",
                "Supply the page text as content.",
            )
        })?;
        let expires_at = crate::civil_date::iso8601_millis_after(self.ttl);
        Ok(json!({"path": readable.canonical, "snapshot": snapshot, "expiresAt": expires_at}))
    }
}
