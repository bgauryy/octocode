//! Waiting for files to be parsed: a server reports a parse by publishing the
//! file's diagnostics.

use super::NativeLspClient;
use crate::lsp::uri::path_to_uri;
use std::collections::HashSet;

impl NativeLspClient {
    /// Which of `files` the server has parsed. Waits for all of them at once,
    /// for as long as the server works: a window of `quiet_ms` with no sign of
    /// work (CPU time, a work-done notice, any message, another file's report
    /// included) ends the wait, as does the caller's deadline; a cancel drops
    /// it. An idle server is parsing none of them, so it costs one window
    /// however many files wait. Every file is then checked without waiting, so
    /// one reported while another was waited for counts; a file left out is
    /// unproven, never assumed parsed.
    pub async fn wait_for_parses(&self, files: Vec<String>, quiet_ms: u32) -> HashSet<String> {
        let _activity = self.lease();
        let Ok(connection) = self.connection_handle().await else {
            return HashSet::new();
        };
        let documents = {
            let open_docs = self
                .inner
                .open_docs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            files
                .iter()
                .filter_map(|file| {
                    let uri = path_to_uri(file).ok()?;
                    let version = open_docs.version(&uri).map(i64::from);
                    Some((file, uri, version))
                })
                .collect::<Vec<_>>()
        };
        let waited = documents
            .iter()
            .map(|(_, uri, version)| (uri.clone(), *version))
            .collect::<Vec<_>>();
        let reports = connection
            .wait_for_push_diagnostics(&waited, quiet_ms)
            .await;
        documents
            .into_iter()
            .zip(reports)
            .filter(|(_, report)| report.is_some())
            .map(|((file, _, _), _)| file.clone())
            .collect()
    }
}
