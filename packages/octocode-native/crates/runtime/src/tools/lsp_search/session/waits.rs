//! Telling the caller what a call waited on: a cold language server costs
//! seconds, and the answer should say so rather than leave the delay
//! unexplained.

use octocode_engine::lsp::client::ServerWaits;
use octocode_engine::lsp::types::LspLanguageServerConfig;
use std::path::Path;
use std::time::Duration;

/// The notes for what this call waited on at the server `config` runs: one
/// per wait, each saying the later calls reuse the running server.
pub(in crate::tools::lsp_search) fn disclose_waits(
    config: &LspLanguageServerConfig,
    waits: ServerWaits,
) -> Vec<String> {
    let server = Path::new(&config.command).file_name().map_or_else(
        || config.command.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    let seconds = |waited: Duration| format!("{:.1} s", waited.as_secs_f64());
    let mut notes = Vec::new();
    if let Some(waited) = waits.starting {
        notes.push(format!(
            "This call waited {} for the {server} language server to start; later calls reuse the running server without that wait.",
            seconds(waited)
        ));
    }
    if let Some(waited) = waits.indexing {
        notes.push(format!(
            "This call waited {} for the {server} language server to load the project; later calls reuse the loaded server without that wait.",
            seconds(waited)
        ));
    }
    notes
}

/// `exit`, when it is a row, with the notes of what the call waited on.
pub(in crate::tools::lsp_search) fn disclose_exit(
    exit: super::Exit,
    config: &LspLanguageServerConfig,
    waits: ServerWaits,
) -> super::Exit {
    match exit {
        super::Exit::Row(mut row) => {
            crate::response::paging::push_warnings(&mut row, disclose_waits(config, waits));
            super::Exit::Row(row)
        }
        other => other,
    }
}
