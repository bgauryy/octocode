use crate::bindings::tasks::SearchRipgrepTask;
use crate::types::{RipgrepParseOptions, RipgrepParseResult, RipgrepSearchOptions};
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Parse ripgrep `--json` NDJSON stdout into structured files + stats in a
/// single `serde_json` streaming pass with no per-line schema validation.
#[napi(js_name = "parseRipgrepJson")]
pub fn parse_ripgrep_json(
    stdout: String,
    options: Option<RipgrepParseOptions>,
) -> RipgrepParseResult {
    crate::portable::parse_ripgrep_json(&stdout, options)
}

/// Run ripgrep in-process: walk `path`, search every file with ripgrep's own
/// engine, and return the same `{ files, stats }` shape as the `--json` parser.
///
/// Runs on the libuv thread pool so the filesystem walk never blocks the event
/// loop.
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
) -> crate::portable::RipgrepPatternValidationResult {
    crate::portable::validate_ripgrep_pattern(
        &pattern,
        fixed_string.unwrap_or(false),
        perl_regex.unwrap_or(false),
    )
}
