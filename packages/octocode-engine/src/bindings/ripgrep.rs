use crate::bindings::tasks::SearchRipgrepTask;
use crate::types::{RipgrepParseOptions, RipgrepParseResult, RipgrepSearchOptions};
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Parse ripgrep `--json` NDJSON stdout into structured files + stats.
///
/// Replaces the TypeScript `parseRipgrepJson` (utils/parsers/ripgrep.ts) which
/// used `JSON.parse` + Zod `safeParse` per NDJSON line and a `[...value]`
/// UTF-16 spread per match snippet. A single `serde_json` streaming pass with
/// no per-line schema validation.
#[napi(js_name = "parseRipgrepJson")]
pub fn parse_ripgrep_json(
    stdout: String,
    options: Option<RipgrepParseOptions>,
) -> RipgrepParseResult {
    crate::search::ripgrep_parser::parse_ripgrep_json_inner(&stdout, options)
}

/// Run ripgrep in-process: walk `path`, search every file with ripgrep's own
/// engine, and return the same `{ files, stats }` shape the `--json` parser
/// produced. Replaces shelling out to an `rg` binary (and the `@vscode/ripgrep`
/// bundle) — octocode is now its own source of ripgrep.
///
/// Runs on the libuv thread pool so the filesystem walk never blocks the event
/// loop, mirroring the old async `spawn` of `rg`.
#[napi(js_name = "searchRipgrep")]
pub fn search_ripgrep(options: RipgrepSearchOptions) -> AsyncTask<SearchRipgrepTask> {
    AsyncTask::new(SearchRipgrepTask {
        options: Some(options),
    })
}

#[napi(js_name = "validateRipgrepPattern")]
pub fn validate_ripgrep_pattern(
    pattern: String,
    fixed_string: Option<bool>,
    perl_regex: Option<bool>,
) -> crate::search::ripgrep_pattern::RipgrepPatternValidationResult {
    crate::search::ripgrep_pattern::validate(
        &pattern,
        fixed_string.unwrap_or(false),
        perl_regex.unwrap_or(false),
    )
}
