//! Telling the caller what a call waited on: a cold language server costs
//! seconds, and the answer should say so rather than leave the delay
//! unexplained.

use octocode_engine::lsp::client::ServerWaits;
use octocode_engine::lsp::types::LspLanguageServerConfig;
use std::time::Duration;

/// The notes for what this call waited on at the server `config` runs: one
/// per wait, each saying the later calls reuse the running server.
pub(in crate::tools::lsp_search) fn disclose_waits(
    config: &LspLanguageServerConfig,
    waits: ServerWaits,
) -> Vec<String> {
    let server = config.display_name();
    let seconds = |waited: Duration| format!("{:.1} s", waited.as_secs_f64());
    // A note explains a slow call; under a second there is nothing to explain.
    let noticed = |waited: &Duration| *waited >= Duration::from_secs(1);
    let mut notes = Vec::new();
    if let Some(waited) = waits.starting.filter(noticed) {
        notes.push(format!(
            "This call waited {} for the {server} language server to start; later calls reuse the running server without that wait.",
            seconds(waited)
        ));
    }
    if let Some(waited) = waits.indexing.filter(noticed) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(command: &str, language_id: Option<&str>) -> Vec<String> {
        let config = LspLanguageServerConfig {
            command: command.into(),
            args: Some(vec!["/x/lib/cli.mjs".into(), "--stdio".into()]),
            workspace_root: "/w".into(),
            language_id: language_id.map(str::to_owned),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: Default::default(),
        };
        let waits = ServerWaits {
            starting: Some(Duration::from_secs(2)),
            indexing: None,
        };
        disclose_waits(&config, waits)
    }

    /// An interpreter that runs the server's script is not the server: the
    /// note names the language it answers for, and otherwise the command.
    #[test]
    fn a_wait_names_the_server_not_the_interpreter_that_runs_it() {
        for (command, language, named) in [
            (
                "/opt/bin/node",
                Some("typescript"),
                "the typescript language",
            ),
            ("python3.12", Some("python"), "the python language"),
            (
                "/usr/bin/rust-analyzer",
                Some("rust"),
                "the rust-analyzer language",
            ),
            ("/usr/bin/clangd-18", Some("cpp"), "the clangd-18 language"),
            ("node", None, "the node language"),
        ] {
            let notes = notes(command, language);
            assert!(notes[0].contains(named), "{notes:?}");
        }
    }
}
