//! The engine walk a local search query asks for.
use crate::tools::local_search::local_search_query::Multiline;
use crate::tools::local_search::local_search_query::Unique;
use crate::tools::local_search::local_search_query::View;
use octocode_contract_types::CaseMode;
use octocode_contract_types::LocalSearchQuery;
use octocode_contract_types::RegexEngine;
use octocode_engine::types::SearchSort;
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
    let path_list = matches!(view, View::Files | View::FilesWithout);
    let multiline = local_search_query::multiline(query);
    TextSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: local_search_query::pattern(query).to_owned(),
        terms: None,
        test_code: None,
        test_code_rank: None,
        line_scope: local_search_query::key_path(query)
            .map(crate::tools::local_search::key_scope::line_scope),
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
        max_depth: local_search_query::max_depth(query),
        // The engine owns the relevance order (for a bare-identifier search,
        // source files declaring the name first; then source before
        // test/generated paths, then count, then declaration > code >
        // comment/string hits, then path) so its top-k and cap keep the same
        // survivors.
        sort: if local_search_query::sort(query) == SearchSort::MatchCount && path_list {
            SearchSort::Path
        } else {
            local_search_query::sort(query)
        },
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
            longer_than: crate::tools::source_view::pagination::MIN_BYTE_PAGE,
            margin: crate::tools::source_view::extraction::MATCH_MARGIN,
        }),
        // Each file is matched as a read of it serves it: bytes a read
        // withholds make no hit, count or listing (see `judge`).
        judge: Some(judge(crate::security::SourceLimits::REAL)),
    }
}

/// The judge of every file a local search reads, at the sizes a read of it
/// is judged at (`limits`): a file judged whole is
/// searched as the text its pass serves
/// ([`crate::security::searched_text`]); a file past the whole-source size
/// a read judges whole is searched with each line its streamed read serves
/// differently replaced by that line ([`crate::tools::large_source::changed_lines`]).
/// A file whose own bytes prove that text holds no match is not judged
/// ([`crate::security::meets_written`]).
pub(crate) fn judge(limits: crate::security::SourceLimits) -> octocode_engine::types::SourceJudge {
    let serve = std::sync::Arc::new(move |path: &std::path::Path, file: &std::fs::File| {
        use std::io::{Read, Seek, SeekFrom};
        #[cfg(test)]
        JUDGED_PATHS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(path.to_path_buf());
        let security = crate::security::ContentSecurity::with_limits(limits);
        let limit = security.limits().whole_source_bytes();
        let meta = file.metadata()?;
        let mut handle = file;
        if meta.len() > limit {
            // The walk stops between files: one file's judgment runs whole.
            struct Whole;
            impl crate::execution::cancel::CancellationCheck for Whole {
                fn check(&self) -> Result<(), String> {
                    Ok(())
                }
            }
            let cancel = Whole;
            let changed =
                crate::tools::large_source::changed_lines(path, file, &meta, &security, &cancel)
                    .map_err(std::io::Error::other)?;
            let Some((changed, _)) = changed.filter(|(changed, _)| !changed.is_empty()) else {
                return Ok(None);
            };
            handle.seek(SeekFrom::Start(0))?;
            let mut text = Vec::new();
            crate::tools::large_source::Patched::new(std::io::BufReader::new(handle), &changed)
                .read_to_end(&mut text)?;
            return Ok(Some(text));
        }
        // The search hands the file at its start ([`octocode_engine::types::JudgeSource`]).
        let mut bytes = Vec::new();
        handle.take(limit).read_to_end(&mut bytes)?;
        // Judged by the pass kept of this text, which a row's whole-source
        // check of the same file takes again rather than scanning twice.
        Ok(crate::security::searched_text(&bytes, |text| {
            crate::tools::judged_rows::judged_text(text, path, &security)
        }))
    });
    octocode_engine::types::SourceJudge {
        serve,
        writes: std::sync::Arc::new(crate::security::meets_written),
    }
}

/// Every path a local search's judge judged in this process, in order.
#[cfg(test)]
pub(crate) static JUDGED_PATHS: std::sync::Mutex<Vec<std::path::PathBuf>> =
    std::sync::Mutex::new(Vec::new());
