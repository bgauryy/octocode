//! The engine walk a local search query asks for.
use crate::local_search::local_search_query::Multiline;
use crate::local_search::local_search_query::Unique;
use crate::local_search::local_search_query::View;
use octocode_contract_types::CaseMode;
use octocode_contract_types::LocalSearchQuery;
use octocode_contract_types::RegexEngine;
use octocode_engine::types::{
    CaseRule, PatternSyntax, SearchOutput, SourceValues, SpanValues, TextSearchOptions,
};

use super::super::super::local_search_query;

#[cfg(not(test))]
fn held_limit() -> Option<usize> {
    None
}

#[cfg(test)]
thread_local! {
    static TEST_HELD: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn held_limit() -> Option<usize> {
    TEST_HELD.with(std::cell::Cell::get)
}

/// Searches planned on this thread hold at most `bytes` of evidence until
/// the guard drops (`None`: the process memory budget).
#[cfg(test)]
pub(crate) fn test_held_limit(bytes: Option<usize>) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_HELD.with(|held| held.set(None));
        }
    }
    TEST_HELD.with(|held| held.set(bytes));
    Reset
}

/// The engine walk the query asks for.
pub(crate) fn search_options(
    query: &LocalSearchQuery,
    root: &std::path::Path,
) -> TextSearchOptions {
    let view = local_search_query::view(query);
    let multiline = local_search_query::multiline(query);
    TextSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: local_search_query::pattern(query).to_owned(),
        terms: None,
        test_code: None,
        line_scope: local_search_query::key_path(query)
            .map(crate::local_search::key_scope::line_scope),
        syntax: match local_search_query::regex_mode(query) {
            RegexEngine::Literal => PatternSyntax::Literal,
            RegexEngine::Rust => PatternSyntax::Regex,
            RegexEngine::Pcre2 => PatternSyntax::Pcre2,
        },
        case: match local_search_query::case_mode(query) {
            CaseMode::Smart => CaseRule::Smart,
            CaseMode::Sensitive => CaseRule::Sensitive,
            CaseMode::Insensitive => CaseRule::Insensitive,
        },
        whole_word: local_search_query::whole_word(query),
        invert_match: local_search_query::invert_match(query),
        multiline: Some(multiline != Multiline::Off),
        multiline_dotall: Some(multiline == Multiline::Dotall),
        output: match view {
            View::Files => SearchOutput::FilesWithMatch,
            View::FilesWithout => SearchOutput::FilesWithoutMatch,
            View::CountLines => SearchOutput::CountLines,
            View::CountMatches => SearchOutput::CountMatches,
            View::MatchOnly => SearchOutput::Spans(match local_search_query::unique(query) {
                Unique::Off => SpanValues::Every,
                Unique::List => SpanValues::Unique,
                Unique::Count => SpanValues::Counted,
            }),
            View::Content => SearchOutput::Lines,
        },
        context: local_search_query::context(query),
        lang_type: local_search_query::file_type(query).map(str::to_owned),
        file_contains_all: Some(
            local_search_query::file_has(query)
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        )
        .filter(|terms: &Vec<String>| !terms.is_empty()),
        file_contains_none: Some(
            local_search_query::file_lacks(query)
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        )
        .filter(|terms: &Vec<String>| !terms.is_empty()),
        include: Some(local_search_query::include(query).to_vec())
            .filter(|include| !include.is_empty()),
        // Credential-named files are withheld, and generated files skipped
        // (both counted), by the walk's path filter, not by globs here.
        exclude: Some(local_search_query::exclude(query).to_vec()),
        // The default-excluded directories are pruned by the walk's path
        // filter too (`WalkAdmission`), which keeps the ones git tracks.
        // Each flag lifts its own cause, as in every local walk; the dot
        // entries left out are recorded by the walk's path filter.
        ignore: octocode_engine::types::IgnorePolicy::new(
            local_search_query::include_ignored(query),
            local_search_query::include_hidden(query),
        ),
        // The call's own rules, set where the search is planned.
        global_ignore: octocode_engine::search::walk::GlobalIgnore::default(),
        max_depth: local_search_query::max_depth(query),
        // The engine owns the order: a matchCount order ranks files by their
        // occurrences in every view, a files view's too.
        sort: local_search_query::sort(query),
        sort_reverse: local_search_query::reverse(query),
        // Every matched file is collected, in the engine sort order above:
        // pages are cut from the whole sorted list, so each one is reachable.
        // Use the engine default per-file byte ceiling (skips pathological
        // multi-GB files, surfaced as a fileTooLarge diagnostic).
        max_file_bytes: None,
        // What the scan holds is bounded by the process memory budget, the
        // engine default; past it the scan is refused, never cut.
        max_held_bytes: held_limit(),
        // A scan small enough to store for later pages keeps the digest of
        // each matched file from the read that searched it (see `manifest`).
        values: SourceValues::Deferred,
        inspect_deferred: Some(super::super::super::verify::scan_inspector()),
        // A content row of a line no byte page of a read is sure to hold
        // whole shows the text around each hit, as a read's match walk of
        // that line does; the line whole is that read's.
        row_window: (view == View::Content).then_some(octocode_engine::types::RowWindow {
            longer_than: octocode_tools_shared::source_view::pagination::MIN_BYTE_PAGE,
            margin: octocode_tools_shared::source_view::extraction::MATCH_MARGIN,
        }),
        // Each file is matched as a read of it serves it: bytes a read
        // withholds make no hit, count or listing, nor does a placeholder
        // it writes in their place (see `judge`).
        judge: Some(judge(octocode_runtime_base::security::SourceLimits::REAL)),
        count_masked: true,
    }
}

/// The judge of every file a local search reads, at the sizes a read of it
/// is judged at (`limits`): a file judged whole is
/// searched as the text its pass serves
/// ([`octocode_runtime_base::security::searched_text`]); a file past the whole-source size
/// a read judges whole is searched with each line its streamed read serves
/// differently replaced by that line ([`octocode_tools_shared::large_source::changed_lines`]).
/// Either way the judge names the bytes the pass wrote, which no match takes
/// ([`octocode_engine::types::ServedText`]).
/// A file whose own bytes prove that text holds no match is not judged
/// ([`octocode_runtime_base::security::meets_written`]).
pub(crate) fn judge(
    limits: octocode_runtime_base::security::SourceLimits,
) -> octocode_engine::types::SourceJudge {
    let serve = std::sync::Arc::new(
        move |path: &std::path::Path, file: &std::fs::File, halted: &(dyn Fn() -> bool + Sync)| {
            use std::io::{Read, Seek, SeekFrom};
            #[cfg(test)]
            JUDGED_PATHS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(path.to_path_buf());
            let security = octocode_runtime_base::security::ContentSecurity::with_limits(limits);
            let limit = security.limits().whole_source_bytes();
            let meta = file.metadata()?;
            let mut handle = file;
            if meta.len() > limit {
                // The search's stop (cancelled, or past its bound) ends this
                // file's judgment between its lines, as it ends the walk.
                struct Stopped<'a>(&'a (dyn Fn() -> bool + Sync));
                impl octocode_runtime_base::execution::cancel::CancellationCheck for Stopped<'_> {
                    fn check(&self) -> Result<(), String> {
                        if (self.0)() {
                            Err(String::from("the search was stopped"))
                        } else {
                            Ok(())
                        }
                    }
                }
                let cancel = Stopped(halted);
                let changed = octocode_tools_shared::large_source::changed_lines(
                    path, file, &meta, &security, &cancel,
                )
                .map_err(|message| {
                    // A stopped search is no failure of the file.
                    let kind = if halted() {
                        std::io::ErrorKind::Interrupted
                    } else {
                        std::io::ErrorKind::Other
                    };
                    std::io::Error::new(kind, message)
                })?;
                let Some((changed, _)) = changed.filter(|(changed, _)| !changed.is_empty()) else {
                    return Ok(None);
                };
                handle.seek(SeekFrom::Start(0))?;
                let mut text = Vec::new();
                let mut patched = octocode_tools_shared::large_source::Patched::new(
                    std::io::BufReader::new(handle),
                    &changed,
                );
                patched.read_to_end(&mut text)?;
                let written = patched.written().to_vec();
                return Ok(Some(octocode_engine::types::ServedText { text, written }));
            }
            // The search hands the file at its start ([`octocode_engine::types::JudgeSource`]).
            let mut bytes = Vec::new();
            handle.take(limit).read_to_end(&mut bytes)?;
            Ok(judged_whole(path, &bytes, &security))
        },
    );
    // Bytes held whole, the start of a file that grew while it was searched,
    // are judged as a whole read of a file of those bytes is.
    let held = octocode_engine::types::JudgeHeld {
        limit: limits.whole_source_bytes(),
        serve: std::sync::Arc::new(move |path: &std::path::Path, bytes: &[u8]| {
            let security = octocode_runtime_base::security::ContentSecurity::with_limits(limits);
            judged_whole(path, bytes, &security)
        }),
    };
    octocode_engine::types::SourceJudge {
        serve,
        writes: std::sync::Arc::new(octocode_runtime_base::security::meets_written),
        held: Some(held),
    }
}

/// The text a whole read of `path`, holding `bytes`, serves when its pass
/// replaced some of them: judged by the pass kept of this text, which a
/// row's whole-source check of the same file takes again rather than
/// scanning twice.
fn judged_whole(
    path: &std::path::Path,
    bytes: &[u8],
    security: &octocode_runtime_base::security::ContentSecurity,
) -> Option<octocode_engine::types::ServedText> {
    octocode_runtime_base::security::searched_text(bytes, |text| {
        octocode_tools_shared::judged_rows::judged_text(text, path, security)
    })
}

/// Every path a local search's judge judged in this process, in order.
#[cfg(test)]
pub(crate) static JUDGED_PATHS: std::sync::Mutex<Vec<std::path::PathBuf>> =
    std::sync::Mutex::new(Vec::new());
