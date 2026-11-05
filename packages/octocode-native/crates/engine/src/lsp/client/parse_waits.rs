//! Waiting for files to be parsed: a server reports a parse by publishing the
//! file's diagnostics.

use super::NativeLspClient;
use std::collections::HashSet;
use tokio::time::Duration;

impl NativeLspClient {
    /// Which of `files` the server has parsed, waiting for each in turn, at most
    /// `timeout_ms` each. A wait that saw no report for any document and no
    /// progress from the server ends the rest: an idle server is parsing none of
    /// them, and each would spend the same wait. A busy one keeps its full wait,
    /// file by file. A file left out is unproven, never assumed parsed.
    pub async fn wait_for_parses(&self, files: Vec<String>, timeout_ms: u32) -> HashSet<String> {
        let mut parsed = HashSet::with_capacity(files.len());
        let Ok(connection) = self.connection_handle().await else {
            return parsed;
        };
        let waited = Duration::from_millis(u64::from(timeout_ms));
        for file in files {
            let published = connection.push_diagnostics_published();
            let cpu = connection.server_cpu_time();
            match self
                .get_push_diagnostics(file.clone(), Some(timeout_ms))
                .await
            {
                Ok(Some(_)) => {
                    parsed.insert(file);
                }
                Ok(None)
                    if connection.push_diagnostics_published() == published
                        && !connection.server_progressed_since(cpu, waited) =>
                {
                    break;
                }
                _ => {}
            }
        }
        parsed
    }
}
