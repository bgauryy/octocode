//! The scan a page is cut from: the walk planned and run, or a stored scan
//! reused once it is proved to answer the query.
use crate::tools::local_search::local_search_query::Multiline;
use crate::tools::local_search::local_search_query::Unique;
use crate::tools::local_search::local_search_query::View;
use octocode_contract_types::CaseMode;
use octocode_contract_types::RegexEngine;
use octocode_engine::types::SearchSort;

use super::super::cursor::{fingerprint, malformed_snapshot, query_key, stale_snapshot};
use super::super::pattern::check_pattern;
use super::super::verify::Redaction;

use super::super::local_search_query::{self, MAX_COLLECTED_FILES, effective_match_content_length};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use octocode_contract_types::{ErrorCode, LocalSearchQuery};
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::types::{
    CaseRule, PatternSyntax, SearchOutput, SourceValues, SpanValues, TextSearchOptions,
};
use std::sync::Arc;

/// A scan with what every later stage needs from it.
pub(crate) struct Scanned {
    pub(crate) validated: crate::policy::path::ValidatedPath,
    pub(crate) parsed: octocode_engine::types::TextSearchResult,
    pub(crate) digests: ScanDigests,
    /// Chars one page's rows may take.
    pub(crate) page_budget: usize,
    /// The result's snapshot identity.
    pub(crate) identity: String,
    /// The query key `identity` was derived from.
    pub(crate) query_key: String,
    /// A stored scan's redaction to replay (see [`redact_scan`]).
    pub(crate) stored_redaction: Option<Arc<Redaction>>,
    /// The names a stored scan's cuts priced, or a fresh scan's empty set.
    pub(crate) name_prices: Arc<super::super::manifest::NamePrices>,
    /// A fresh walk offered for storage with the priced, unranked scan.
    pub(crate) reusable: Option<super::super::walked::Walked>,
    /// The query's own walk, for probes over the same scope.
    pub(crate) search_options: TextSearchOptions,
    pub(crate) policy_key: String,
    /// What the walk left out: policy-withheld, ignored, default-excluded
    /// and hidden entries.
    pub(crate) skipped: Arc<crate::policy::discovery::WalkSkips>,
}

/// Plan and run the walk (or reuse its stored scan) and prove a cursor
/// still answers this query.
pub(crate) fn scan_query(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
    walk_threads: Option<u32>,
    response_window: Option<usize>,
) -> Result<Scanned, ToolError> {
    if let Some(error) = malformed_snapshot(query) {
        return Err(error);
    }
    let validated = search_root(query, paths)?;
    let options = search_options(query, &validated.canonical, walk_threads);
    check_pattern(query)?;
    let search_options = options.clone();
    let policy_key = paths.identity();
    let ScanOutput {
        parsed,
        digests,
        skipped,
        stored,
        walked,
    } = scan(query, paths, options, &policy_key, cancel)?;
    // Pages are cut from serialized sizes so a default-layout page, with
    // the row around it, fits one response window. The budget charges the
    // root's spelling a fixed size (`page_windows`), so a continuation that
    // names the same root relative to the workspace cuts the same pages.
    let sized = crate::response::result::query_value(query);
    let page_budget = crate::response::page_windows::page_budget(response_window, &sized);
    // A stored scan was stored under the snapshot derived from its query
    // key and its unchanged collected result: a query with the same key
    // derives that snapshot again, so only another key needs the digest.
    let query_key = query_key(query, &validated.canonical, page_budget);
    let (identity, stored_redaction, name_prices) =
        match (stored, local_search_query::snapshot(query)) {
            (Some(stored), Some(snapshot)) if stored.query_key == query_key => {
                (snapshot.to_owned(), stored.redaction, stored.prices)
            }
            (Some(_), Some(_)) => return Err(stale_snapshot(query)),
            _ => (
                fingerprint(&query_key, &parsed.files, &parsed.stats),
                None,
                Arc::default(),
            ),
        };
    // A cached scan is checked too: its identity comes from the submitted
    // query's key, so a cursor reused with different search semantics never
    // serves the old query's matches.
    if local_search_query::snapshot(query).is_some_and(|expected| expected != identity) {
        return Err(stale_snapshot(query));
    }
    Ok(Scanned {
        validated,
        parsed,
        digests,
        page_budget,
        identity,
        query_key,
        stored_redaction,
        name_prices,
        reusable: walked,
        search_options,
        policy_key,
        skipped,
    })
}

/// The search root under the path policy; a missing one is not-found
/// (exit 3), not an I/O failure, and leads to a tree of its nearest
/// existing parent, where a typo's siblings show.
pub(crate) fn search_root(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
) -> Result<crate::policy::path::ValidatedPath, ToolError> {
    paths
        .validate(local_search_query::path(query))
        .map_err(|error| ToolError::root_policy(error, local_search_query::path(query), paths))
}

/// The engine walk the query asks for.
pub(crate) fn search_options(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    walk_threads: Option<u32>,
) -> TextSearchOptions {
    let view = local_search_query::view(query);
    let path_list = matches!(view, View::Files | View::FilesWithout);
    let multiline = local_search_query::multiline(query);
    TextSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: local_search_query::pattern(query).to_owned(),
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
        exclude_dir: None,
        // Each flag lifts its own cause, as in every local walk; the dot
        // entries left out are recorded by the walk's path filter.
        ignore: octocode_engine::search::walk::IgnorePolicy::new(
            local_search_query::include_ignored(query),
            local_search_query::include_hidden(query),
        ),
        max_depth: local_search_query::max_depth(query),
        // The engine owns the relevance order (for a bare-identifier search,
        // source files declaring the name first; then count, then source
        // before test/generated paths, then declaration > code >
        // comment/string hits, then path) so its top-k and cap keep the same
        // survivors.
        sort: if local_search_query::sort(query) == SearchSort::MatchCount && path_list {
            SearchSort::Path
        } else {
            local_search_query::sort(query)
        },
        sort_reverse: local_search_query::reverse(query),
        max_snippet_chars: Some(effective_match_content_length(query)),
        // The engine keeps the first 10k matched files in the engine sort order
        // above (by match count for relevance/matchCount), chosen across every
        // searched file; stats totals still count all matched files and the
        // cap surfaces as capReason "maxCollectedFiles".
        max_collected_files: Some(MAX_COLLECTED_FILES),
        // Use the engine default per-file byte ceiling (skips pathological
        // multi-GB files, surfaced as a maxFileSize diagnostic).
        max_file_bytes: None,
        // The batch's share of the cores (see `BatchBudget`); `None` walks
        // on every core.
        walk_threads,
        // A scan small enough to store for later pages keeps the digest of
        // each matched file from the read that searched it (see `manifest`).
        values: SourceValues::Deferred,
    }
}

/// A scan and the digests of its matched files when it was stored.
pub(crate) type ScanDigests =
    Option<std::collections::HashMap<std::path::PathBuf, super::super::sources::Digest>>;

/// A walk's matches, the stored digests a reused scan must still hash to,
/// and what the walk left out.
pub(crate) struct ScanOutput {
    pub(crate) parsed: octocode_engine::types::TextSearchResult,
    pub(crate) digests: ScanDigests,
    pub(crate) skipped: Arc<crate::policy::discovery::WalkSkips>,
    /// What was stored with a reused scan: its query key, redaction and
    /// name prices.
    pub(crate) stored: Option<StoredDerived>,
    /// What a fresh walk searched; `None` for a reused scan.
    pub(crate) walked: Option<super::super::walked::Walked>,
}

/// What a stored scan's fresh page derived from it.
pub(crate) struct StoredDerived {
    pub(crate) query_key: String,
    pub(crate) redaction: Option<Arc<Redaction>>,
    pub(crate) prices: Arc<super::super::manifest::NamePrices>,
}

/// The matches: a continuation reuses its page-1 scan instead of walking
/// the tree again. A stored scan is reused only under the path policy that
/// produced it; the snapshot comparison then proves it answers this query.
/// The walk admitted each matched path under the read policy; a page checks
/// again each file it reads or shows (see [`shape_page`]).
pub(crate) fn scan(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: TextSearchOptions,
    policy_key: &str,
    cancel: &impl CancellationCheck,
) -> Result<ScanOutput, ToolError> {
    let (parsed, digests, withheld, derived, walked) = if let Some(snapshot) =
        local_search_query::snapshot(query)
        && let Some(stored) =
            super::super::manifest::get_checked(snapshot, policy_key, &|| cancel.check().is_err())
    {
        let derived = StoredDerived {
            query_key: stored.query_key,
            redaction: stored.redaction,
            prices: stored.prices,
        };
        (
            stored.value,
            Some(stored.digests),
            stored.skipped,
            Some(derived),
            None,
        )
    } else {
        let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
            paths.clone(),
            std::path::PathBuf::from(&options.path),
            local_search_query::default_excludes(query).defaults(),
            options.include.as_deref().unwrap_or_default(),
        ));
        let recorder = Arc::new(super::super::walked::Recorder::new(
            Arc::clone(&walk),
            paths.clone(),
            std::path::PathBuf::from(&options.path),
            local_search_query::include_ignored(query) != Some(true),
            super::super::manifest::MAX_BYTES,
        ));
        let parsed = search_cancellable(options, recorder.clone(), &|| cancel.check().is_err())
            .map_err(|error| {
                // A malformed glob or file type is caller input.
                let code = match error.kind() {
                    octocode_engine::error::ErrorKind::InvalidInput => ErrorCode::InvalidInput,
                    octocode_engine::error::ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
                    _ => ErrorCode::ExecutionFailed,
                };
                ToolError::new(code, error.to_string())
            })?;
        (
            parsed,
            None,
            Arc::new(walk.skipped()),
            None,
            Some(recorder.walked()),
        )
    };
    cancel.check().map_err(ToolError::cancelled)?;
    if parsed.files.is_empty()
        && parsed.stats.files_searched.unwrap_or(0) == 0
        && parsed.stats.error_count.unwrap_or(0) > 0
    {
        return Err(unreadable_scope(&parsed.stats));
    }
    Ok(ScanOutput {
        parsed,
        digests,
        skipped: withheld,
        stored: derived,
        walked,
    })
}

/// Every candidate failed before it could be searched: there is no evidence,
/// so this is an execution failure, not an empty result.
pub(crate) fn unreadable_scope(stats: &octocode_engine::types::TextSearchStats) -> ToolError {
    let count = stats.error_count.unwrap_or(0);
    let first = stats.first_error.as_deref().unwrap_or("unknown error");
    // Every file was readable; the regex failed in each (a backtracking or
    // JIT-stack limit): the pattern, not the scope, needs the change.
    if stats.pattern_error_count == Some(count) {
        return ToolError::new(
            ErrorCode::InvalidPattern,
            format!(
                "The regex hit the engine's matching limit in every file ({count}; first: {first}). Simplify nested repetition, or use regex:\"rust\" (no backtracking)."
            ),
        );
    }
    ToolError {
        ..ToolError::new(
            ErrorCode::FileAccessFailed,
            format!(
                "No file under the search path could be read ({count} failure(s); first: {first})."
            ),
        )
    }
}
