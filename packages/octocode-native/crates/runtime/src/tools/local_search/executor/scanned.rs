//! The scan a page is cut from: the walk planned and run, or a stored scan
//! reused once it is proved to answer the query.
mod options;

#[cfg(test)]
pub(crate) use options::test_held_limit;
pub(crate) use options::{judge, search_options};

use super::super::cursor::{fingerprint, malformed_snapshot, query_key, stale_snapshot};
use super::super::manifest::StoredValue;
use super::super::pattern::check_pattern;
use super::super::verify::Redaction;

use super::super::local_search_query;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use octocode_contract_types::{ErrorCode, LocalSearchQuery};
use octocode_engine::search::text_search::{Pause, SearchWindow, Windowed, search_window};
use octocode_engine::types::TextSearchOptions;
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
    /// What a stored scan's pages learned, or a fresh scan's empty set.
    pub(crate) learned: Arc<super::super::manifest::Learned>,
    /// A fresh walk offered for storage with the priced, unranked scan.
    pub(crate) reusable: Option<super::super::walked::Walked>,
    /// The query's own walk, for probes over the same scope.
    pub(crate) search_options: TextSearchOptions,
    pub(crate) policy_key: String,
    /// What the walk left out: policy-withheld, ignored, default-excluded
    /// and hidden entries.
    pub(crate) skipped: Arc<crate::policy::discovery::WalkSkips>,
    /// The search paused at its window's end; `parsed` holds no file then
    /// ([`super::super::window`]).
    pub(crate) paused: Option<Pause>,
    /// A stored scan read by page; `parsed` holds only its stats then.
    pub(crate) seek: Option<Arc<super::super::packed::Packed>>,
}

/// [`scan_resumed`] of a query that resumes no paused search.
#[cfg(test)]
pub(crate) fn scan_query(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
    (response_window, terms): (Option<usize>, Option<Vec<String>>),
) -> Result<Scanned, ToolError> {
    scan_resumed(
        query,
        (paths, crate::security::SourceLimits::REAL),
        cancel,
        (response_window, terms),
        None,
    )
}

/// Plan and run the walk (or reuse its stored scan) and prove a cursor
/// still answers this query, judging each file at `limits`. `terms` are the `matchString` entries a
/// normalized query joined (see [`local_search_query::term_patterns`]):
/// a count view counts each. `resumed` is where a PCRE2 search paused by an
/// earlier window resumes ([`super::super::window::resumed`]).
pub(crate) fn scan_resumed(
    query: &LocalSearchQuery,
    (paths, limits): (&PathPolicy, crate::security::SourceLimits),
    cancel: &impl CancellationCheck,
    (response_window, terms): (Option<usize>, Option<Vec<String>>),
    resumed: Option<super::super::window::Resumed>,
) -> Result<Scanned, ToolError> {
    if let Some(error) = malformed_snapshot(query) {
        return Err(error);
    }
    let validated = search_root(query, paths)?;
    let keep = crate::tools::test_code::Keep::of(local_search_query::test_code(query));
    let verdicts = keep.map(|_| Arc::new(crate::tools::test_code::TestCodeIndex::new(paths)));
    let options = TextSearchOptions {
        terms,
        test_code: keep
            .zip(verdicts.clone())
            .map(|(keep, index)| crate::tools::test_code::text_scope(keep, index)),
        // Each file is judged at the sizes a read of it is.
        judge: Some(judge(limits)),
        ..search_options(query, &validated.canonical)
    };
    check_pattern(query)?;
    let search_options = options.clone();
    let policy_key = paths.identity();
    // Pages are cut from serialized sizes so a default-layout page, with
    // the row around it, fits one response window. The budget charges the
    // root's spelling a fixed size (`page_windows`), so a continuation that
    // names the same root relative to the workspace cuts the same pages.
    let page_budget = crate::response::page_windows::page_budget(response_window);
    let query_key = query_key(query, &validated.canonical, page_budget);
    let window = super::super::window::window(query, resumed, (&query_key, &policy_key), cancel);
    let ScanOutput {
        mut parsed,
        digests,
        skipped,
        stored,
        mut walked,
        paused,
        seek,
    } = scan_in(query, paths, options, &policy_key, cancel, window)?;
    // What a `testCode` scope's verdicts read, a parent module outside this
    // root too, proves a stored scan as the walk's ignore inputs do, past what
    // the walk already stamps.
    if let (Some(walked), Some(verdicts)) = (&mut walked, &verdicts) {
        let stamped = walked
            .entries
            .iter()
            .collect::<std::collections::HashSet<_>>();
        let watched = verdicts
            .watched()
            .into_iter()
            .filter(|path| !stamped.contains(path))
            .collect::<Vec<_>>();
        walked.rules.extend(watched);
    }
    // A verdict that needed a file it could not read is unproven: the row
    // says so as for any other path it could not read.
    if let Some(verdicts) = &verdicts {
        disclose_unread(&mut parsed.stats, &verdicts.unread());
    }
    // A stored scan was stored under the snapshot derived from its query
    // key and its unchanged collected result: a query with the same key
    // derives that snapshot again, so only another key needs the digest.
    let (identity, stored_redaction, learned) = match (stored, local_search_query::snapshot(query))
    {
        (Some(stored), Some(snapshot)) if stored.query_key == query_key => {
            (snapshot.to_owned(), stored.redaction, stored.learned)
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
        learned,
        reusable: walked,
        search_options,
        policy_key,
        skipped,
        paused,
        seek,
    })
}

/// Counts as failed paths the Rust files a `testCode` verdict could not read.
fn disclose_unread(
    stats: &mut octocode_engine::types::TextSearchStats,
    unread: &[std::path::PathBuf],
) {
    let Some(note) = crate::tools::test_code::unread_note(unread) else {
        return;
    };
    let count = octocode_engine::num::saturating_u32(unread.len());
    stats.error_count = Some(stats.error_count.unwrap_or(0).saturating_add(count));
    stats.first_error.get_or_insert(note);
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

/// A scan and the digests of its matched files when it was stored.
pub(crate) type ScanDigests =
    Option<Arc<std::collections::HashMap<std::path::PathBuf, super::super::sources::Digest>>>;

/// A walk's matches, the stored digests a reused scan must still hash to,
/// and what the walk left out.
pub(crate) struct ScanOutput {
    pub(crate) parsed: octocode_engine::types::TextSearchResult,
    pub(crate) digests: ScanDigests,
    pub(crate) skipped: Arc<crate::policy::discovery::WalkSkips>,
    /// What was stored with a reused scan: its query key, redaction and
    /// what its pages learned.
    pub(crate) stored: Option<StoredDerived>,
    /// What a fresh walk searched; `None` for a reused scan or a paused one.
    pub(crate) walked: Option<super::super::walked::Walked>,
    /// The search paused at its window's end.
    pub(crate) paused: Option<Pause>,
    /// A stored scan read by page (see [`Scanned::seek`]).
    pub(crate) seek: Option<Arc<super::super::packed::Packed>>,
}

/// What a stored scan's fresh page derived from it.
pub(crate) struct StoredDerived {
    pub(crate) query_key: String,
    pub(crate) redaction: Option<Arc<Redaction>>,
    pub(crate) learned: Arc<super::super::manifest::Learned>,
}

/// The matches: a continuation reuses its page-1 scan instead of walking
/// the tree again. A stored scan is reused only under the path policy that
/// produced it; the snapshot comparison then proves it answers this query.
/// The walk admitted each matched path under the read policy; a page checks
/// again each file it reads or shows (see [`shape_page`]).
#[cfg(test)]
pub(crate) fn scan(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: TextSearchOptions,
    policy_key: &str,
    cancel: &impl CancellationCheck,
) -> Result<ScanOutput, ToolError> {
    scan_in(
        query,
        paths,
        options,
        policy_key,
        cancel,
        SearchWindow::default(),
    )
}

/// [`scan`] in `window`: a fresh PCRE2 walk may pause at its end.
pub(crate) fn scan_in(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: TextSearchOptions,
    policy_key: &str,
    cancel: &impl CancellationCheck,
    window: SearchWindow,
) -> Result<ScanOutput, ToolError> {
    let mut seek = None;
    let (parsed, digests, withheld, derived, walked, paused) = if let Some(snapshot) =
        local_search_query::snapshot(query)
        && let Some(stored) =
            super::super::manifest::get_checked(snapshot, policy_key, &|| cancel.check().is_err())
                .filter(|stored| serves(&stored.value, query))
    {
        let derived = StoredDerived {
            query_key: stored.query_key,
            redaction: stored.redaction,
            learned: stored.learned,
        };
        let parsed = match stored.value {
            StoredValue::Whole(parsed) => *parsed,
            StoredValue::Seek(packed) => {
                let stats = packed.stats().clone();
                seek = Some(packed);
                octocode_engine::types::TextSearchResult {
                    files: Vec::new(),
                    stats,
                }
            }
        };
        (
            parsed,
            Some(stored.digests),
            stored.skipped,
            Some(derived),
            None,
            None,
        )
    } else {
        let root = std::path::PathBuf::from(&options.path);
        let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
            paths.clone(),
            root.clone(),
            local_search_query::default_excludes(query).defaults(),
            options.include.as_deref().unwrap_or_default(),
        ));
        let recorder = Arc::new(super::super::walked::Recorder::new(
            Arc::clone(&walk),
            paths.clone(),
            std::path::PathBuf::from(&options.path),
            local_search_query::include_ignored(query) != Some(true),
            octocode_engine::memory::MEMORY.limit(),
        ));
        let windowed = search_window(
            options,
            recorder.clone(),
            &|| cancel.check().is_err(),
            window,
        )
        .map_err(|error| {
            // A malformed glob or file type is caller input.
            let code = match error.kind() {
                octocode_engine::error::ErrorKind::InvalidInput => ErrorCode::InvalidInput,
                octocode_engine::error::ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
                octocode_engine::error::ErrorKind::ResourceExhausted => {
                    return held_past_budget(query, &error, root.is_file());
                }
                _ => ErrorCode::ExecutionFailed,
            };
            ToolError::new(code, error.to_string())
        })?;
        let skipped = Arc::new(walk.skipped());
        match windowed {
            Windowed::Whole(parsed) => (parsed, None, skipped, None, Some(recorder.walked()), None),
            Windowed::Paused(pause) => (
                octocode_engine::types::TextSearchResult {
                    files: Vec::new(),
                    stats: octocode_engine::types::TextSearchStats::default(),
                },
                None,
                skipped,
                None,
                None,
                Some(pause),
            ),
        }
    };
    cancel.check().map_err(ToolError::cancelled)?;
    if paused.is_none()
        && seek.is_none()
        && parsed.files.is_empty()
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
        paused,
        seek,
    })
}

/// Whether a stored scan can serve `query`'s page. A whole scan is cut
/// again for any page. A seekable one holds only the pages of its own
/// streamed cut: a classification page judges every row before it cuts,
/// and a match-page grid (no public call reaches one, see
/// [`super::super::packed::PageIndex`]) cuts other pages, so either walks
/// again (the same snapshot then comes from the fresh scan).
fn serves(value: &StoredValue, query: &LocalSearchQuery) -> bool {
    match value {
        StoredValue::Whole(_) => true,
        StoredValue::Seek(_) => {
            !local_search_query::has_classification(query)
                && local_search_query::streamed_layout(query)
        }
    }
}

/// A scan whose matches passed the process memory budget before it ended:
/// every order ranks the whole scope and every page is cut from the whole
/// result, so no page of it is shown (none would be complete). It is
/// refused with the calls that answer within the budget. A `one_file` scope
/// is read in pages by localFetch (its count and listing would only name
/// the same file, whose search is this refusal). A directory gets the same
/// search as per-file counts, which keep no rows (from a row view), and a
/// listing of the scope to split it by.
fn held_past_budget(
    query: &LocalSearchQuery,
    error: &octocode_engine::error::Error,
    one_file: bool,
) -> ToolError {
    use crate::tools::local_search::{layout::list_view, leads};
    let mut calls = serde_json::Map::new();
    let hint = if one_file {
        calls.insert(
            "readFile".into(),
            crate::response::result::Continuation::related(
                crate::id::ToolId::LocalFetch,
                serde_json::json!({"path": local_search_query::path(query)}),
            )
            .build(),
        );
        ONE_FILE_HINT.to_owned()
    } else {
        if !list_view(local_search_query::view(query))
            && let Ok(lead) = leads::count_lead(query)
        {
            calls.insert("viewFileCount".into(), lead);
        }
        if let Some(lead) = leads::scope_listing(query, true) {
            calls.insert("listScope".into(), lead);
        }
        directory_hint(
            calls.contains_key("viewFileCount"),
            calls.contains_key("listScope"),
        )
    };
    ToolError {
        hints: vec![hint],
        calls: (!calls.is_empty()).then(|| Box::new(serde_json::Value::Object(calls))),
        ..ToolError::new(
            ErrorCode::ScopeTooBroad,
            format!(
                "The search was refused: {error}; every order ranks the whole scope, so no page is cut from a partial scan."
            ),
        )
    }
}

/// The refusal hint for a one-file scope, whose only lead is its read.
const ONE_FILE_HINT: &str = "Nothing was shown: run the related[] localFetch call (it pages the file whole) or narrow the pattern.";

/// The refusal hint for a directory scope, naming only the leads offered: a
/// per-file count (`counted`) and a listing of the scope (`listed`), each by
/// the tool its related[] call runs (call keys never reach the output).
fn directory_hint(counted: bool, listed: bool) -> String {
    let leads = match (counted, listed) {
        (true, true) => {
            "run the related[] localSearch call for per-file counts or the structureSearch listing to split the scope, or "
        }
        (true, false) => "run the related[] localSearch call for per-file counts, or ",
        (false, true) => "run the related[] structureSearch listing to split the scope, or ",
        (false, false) => "",
    };
    format!("Nothing was shown: {leads}narrow path, include or the pattern.")
}

/// Every candidate failed before it could be searched: there is no evidence,
/// so this is an execution failure, not an empty result.
pub(crate) fn unreadable_scope(stats: &octocode_engine::types::TextSearchStats) -> ToolError {
    let count = stats.error_count.unwrap_or(0);
    let first = stats.first_error.as_deref().unwrap_or("unknown error");
    // Every file was readable; the regex failed in each (a backtracking or
    // JIT-stack limit): the pattern, not the scope, needs the change. It
    // compiled, so it is an execution failure, not `invalidPattern`.
    if stats.pattern_error_count == Some(count) {
        return ToolError::new(
            ErrorCode::ExecutionFailed,
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

#[cfg(test)]
mod hint_tests {
    /// Call keys never reach the output (`attach_calls` lists a related
    /// call in `related[]` without its key), so a hint naming one points
    /// at nothing.
    const INTERNAL_KEYS: [&str; 3] = ["readFile", "viewFileCount", "listScope"];

    /// Each lead the hint names is one the refusal offers, and only those,
    /// named by the tool its related[] call runs.
    #[test]
    fn a_directory_hint_names_only_offered_leads_by_tool() {
        for counted in [false, true] {
            for listed in [false, true] {
                let hint = super::directory_hint(counted, listed);
                assert_eq!(
                    hint.contains("related[] localSearch call for per-file counts"),
                    counted,
                    "{hint}"
                );
                assert_eq!(
                    hint.contains("structureSearch listing to split the scope"),
                    listed,
                    "{hint}"
                );
                assert!(
                    hint.contains("narrow path, include or the pattern"),
                    "{hint}"
                );
                for key in INTERNAL_KEYS {
                    assert!(!hint.contains(key), "{hint}");
                }
            }
        }
    }

    /// A regex that compiles but hits the engine's matching limit in every
    /// file is an execution failure, not an invalid pattern.
    #[test]
    fn a_matching_limit_in_every_file_is_an_execution_failure() {
        let stats = octocode_engine::types::TextSearchStats {
            error_count: Some(2),
            pattern_error_count: Some(2),
            first_error: Some("a.txt: PCRE2: error matching: match limit exceeded".into()),
            ..Default::default()
        };
        let error = super::unreadable_scope(&stats);
        assert_eq!(error.code, super::ErrorCode::ExecutionFailed, "{error:?}");
        assert!(error.message.contains("matching limit"), "{error:?}");
    }

    /// A one-file refusal names its localFetch lead by tool.
    #[test]
    fn a_one_file_hint_names_the_localfetch_call() {
        let hint = super::ONE_FILE_HINT;
        assert!(
            hint.contains("related[] localFetch call (it pages the file whole)"),
            "{hint}"
        );
        for key in INTERNAL_KEYS {
            assert!(!hint.contains(key), "{hint}");
        }
    }
}
