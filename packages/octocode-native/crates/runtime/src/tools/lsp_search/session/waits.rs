//! Telling the caller what a call waited on: a cold language server costs
//! seconds, and the answer should say so rather than leave the delay
//! unexplained.

use octocode_engine::lsp::client::ServerWaits;
use octocode_engine::lsp::types::LspLanguageServerConfig;
use std::time::Duration;

/// The waits this call measured at the server `config` runs: each a note
/// (one `{}` where the seconds go) with its span, saying the later calls reuse
/// the running server. The response stage states each note once for the call
/// ([`crate::response::paging::push_timed_notice`]).
pub(in crate::tools::lsp_search) fn disclose_waits(
    config: &LspLanguageServerConfig,
    waits: ServerWaits,
) -> Vec<(String, Duration)> {
    let server = config.display_name();
    // A note explains a slow call; under a second there is nothing to explain.
    let noticed = |waited: &Duration| *waited >= Duration::from_secs(1);
    let mut notes = Vec::new();
    if let Some(waited) = waits.starting.filter(noticed) {
        notes.push((
            format!(
                "This call waited {{}} for the {server} language server to start; later calls reuse the running server without that wait."
            ),
            waited,
        ));
    }
    if let Some(waited) = waits.indexing.filter(noticed) {
        notes.push((
            format!(
                "This call waited {{}} for the {server} language server to load the project; later calls reuse the loaded server without that wait."
            ),
            waited,
        ));
    }
    notes
}

/// The warning for a pool that holds more resident memory than its budget
/// after this call's acquire (`resident`, `budget` in bytes): the pool never
/// stops a server that is busy with a request, so it stays over until those
/// servers are idle.
pub(in crate::tools::lsp_search) fn disclose_pool_overflow(resident: u64, budget: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    format!(
        "The language server pool holds about {} MiB, over its {} MiB budget, because servers busy with requests are never stopped; the pool stops them once they are idle.",
        resident.div_ceil(MIB),
        budget.div_ceil(MIB)
    )
}

/// Records each of `waits` on `row`.
pub(in crate::tools::lsp_search) fn record_waits(
    row: &mut impl crate::response::paging::RowFields,
    waits: Vec<(String, Duration)>,
) {
    for (text, waited) in waits {
        crate::response::paging::push_timed_notice(row, &text, waited.as_secs_f64());
    }
}

/// `exit`, when it is a row, with the waits of what the call waited on.
pub(in crate::tools::lsp_search) fn disclose_exit(
    exit: super::Exit,
    config: &LspLanguageServerConfig,
    waits: ServerWaits,
) -> super::Exit {
    match exit {
        super::Exit::Row(mut row) => {
            record_waits(&mut row, disclose_waits(config, waits));
            super::Exit::Row(row)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_overflow_warning_states_both_sizes_and_why() {
        let mib = 1024 * 1024;
        let warning = disclose_pool_overflow(300 * mib + 1, 200 * mib);
        assert!(warning.contains("about 301 MiB"), "{warning}");
        assert!(warning.contains("over its 200 MiB budget"), "{warning}");
        assert!(
            warning.contains("busy with requests are never stopped"),
            "{warning}"
        );
        assert!(warning.contains("once they are idle"), "{warning}");
    }

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
            .into_iter()
            .map(|(text, _)| text)
            .collect()
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

    /// Three rows of one call that waited on a loading server for 46.1, 46.1
    /// and 28.5 s: the wait is one fact of the call, stated once in the
    /// envelope with the longest span, and no row carries it.
    #[test]
    fn a_wait_rows_measure_differently_is_stated_once_for_the_call() {
        let config = LspLanguageServerConfig {
            command: "/usr/bin/rust-analyzer".into(),
            args: None,
            workspace_root: "/w".into(),
            language_id: Some("rust".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: Default::default(),
        };
        let rows = [46_100, 46_100, 28_500]
            .into_iter()
            .map(|millis| {
                let mut data = serde_json::json!({"matches": []});
                let waits = ServerWaits {
                    starting: None,
                    indexing: Some(Duration::from_millis(millis)),
                };
                record_waits(&mut data, disclose_waits(&config, waits));
                data
            })
            .collect::<Vec<_>>();
        let response =
            crate::tools::test_support::staged_response(crate::id::ToolId::LspSearch, rows);
        let warnings = response["warnings"].as_array().expect("envelope warnings");
        assert_eq!(warnings.len(), 1, "{response}");
        let note = warnings[0].as_str().expect("text");
        assert!(
            note.starts_with("This call waited 46.1 s for the rust-analyzer language server"),
            "{note}"
        );
        for (index, row) in response["results"]
            .as_array()
            .expect("rows")
            .iter()
            .enumerate()
        {
            let data = &row["data"];
            assert!(data.get("warnings").is_none(), "row {index}: {response}");
            assert!(
                data.get("timedNotices").is_none(),
                "row {index}: {response}"
            );
        }
    }
}
