//! A `regex:"pcre2"` search in windows of its call's budget.
//!
//! Nothing bounds the time of one PCRE2 match, so a first page's PCRE2
//! search ends its window at the midpoint of what its call has left
//! ([`octocode_runtime_base::execution::cancel::halfway`]), leaving the other half to answer.
//! Past it the engine pauses after the last file it finished in walk order
//! ([`octocode_engine::search::text_search::Pause`]). Every order ranks the
//! whole scope, so a paused window lists no file: it keeps what it searched
//! and answers `next.nextPage`, the same query with `scanOffset` (where the
//! search resumes, in walk order) and `snapshot` (the pin of the files before
//! it). The window that ends the search answers as the query without them
//! does: the same files, counts and snapshot as a search that never paused.
//!
//! What a paused window kept is a candidate: it serves while the files it
//! passed are the same, unchanged (the engine compares the pin). Otherwise,
//! or when it is gone, the window searches those files again before its own
//! end applies, so every window makes progress.
use super::cursor::{SNAPSHOT_SCHEME, malformed_snapshot};
use super::local_search_query;
use super::types::{LocalSearchResult, SearchStats, SearchStatus};
use octocode_contract_types::{LocalSearchQuery, RegexEngine};
use octocode_engine::search::text_search::{Earlier, Pause, Resume, SearchWindow, Searched};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::paging::PartialReason;
use octocode_runtime_base::response::result::{Continuation, ToolError};
use octocode_tools_shared::page_memo::PageMemo;
use serde_json::json;
use std::borrow::Cow;
use std::sync::Mutex;

/// Where a query resumes a paused search: `scanOffset` and the pin its
/// `snapshot` carries.
pub(super) struct Resumed {
    position: u32,
    pin: Option<String>,
}

/// The query's resume, and the query without it: the one every later stage
/// answers, so the window that ends the search pages as a first page does.
pub(super) fn resumed(
    query: &LocalSearchQuery,
) -> Result<(Option<Resumed>, Cow<'_, LocalSearchQuery>), ToolError> {
    // `scanOffset` of a linear engine counts the rows a streamed page
    // listed ([`super::executor::streamed`]); only a PCRE2 search resumes.
    let Some(position) = local_search_query::scan_offset(query)
        .filter(|_| local_search_query::regex_mode(query) == RegexEngine::Pcre2)
    else {
        return Ok((None, Cow::Borrowed(query)));
    };
    if let Some(error) = malformed_snapshot(query) {
        return Err(error);
    }
    let pin = local_search_query::snapshot(query).map(|snapshot| {
        snapshot
            .split_once(':')
            .map_or(snapshot, |(_, digest)| digest)
            .to_owned()
    });
    let plain = local_search_query::edited(query, &json!({"scanOffset": null, "snapshot": null}))?;
    Ok((Some(Resumed { position, pin }), Cow::Owned(plain)))
}

/// What the paused windows of one search kept, taken by the window that
/// resumes it.
static STANDINGS: PageMemo<Mutex<Option<Searched>>> = PageMemo::new();

fn key(query_key: &str, position: u32, pin: &str) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        "localSearch.standing",
        query_key,
        position,
        pin
    ]))
}

/// The window a scan of `query` runs: a PCRE2 first page ends at the
/// midpoint of its call's budget; a later page (whose scan answers pages an
/// earlier call cut) and a linear engine run whole.
pub(super) fn window(
    query: &LocalSearchQuery,
    resumed: Option<Resumed>,
    (query_key, policy): (&str, &str),
    cancel: &dyn CancellationCheck,
) -> SearchWindow {
    let first_page =
        local_search_query::page(query) <= 1 && local_search_query::snapshot(query).is_none();
    if local_search_query::regex_mode(query) != RegexEngine::Pcre2 || !first_page {
        return SearchWindow::default();
    }
    let resume = resumed.map(|Resumed { position, pin }| Resume {
        position,
        earlier: pin.and_then(|pin| {
            let key = key(query_key, position, &pin);
            let searched = STANDINGS.get(&key, policy, |kept| {
                kept.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
            })??;
            STANDINGS.evict(&key);
            Some(Earlier { pin, searched })
        }),
    });
    #[cfg(test)]
    if let Some(window) = test_window() {
        return SearchWindow { resume, ..window };
    }
    SearchWindow {
        ends: octocode_runtime_base::execution::cancel::halfway(cancel),
        clock: None,
        resume,
    }
}

/// Keep what a paused window searched for the window that resumes it, and
/// answer its row: no file, `scanBudget`, and `next.nextPage` there.
pub(super) fn paused_row(
    build: &str,
    query: &LocalSearchQuery,
    pause: Pause,
    (query_key, policy): (&str, &str),
) -> Result<LocalSearchResult, ToolError> {
    let Pause {
        position,
        pin,
        searched,
    } = pause;
    let snapshot = format!("{SNAPSHOT_SCHEME}{}:{pin}", build);
    let resume = local_search_query::edited(
        query,
        &json!({"scanOffset": position, "snapshot": snapshot, "page": null}),
    )?;
    let saturating = octocode_engine::num::saturating_u32;
    // Counted in the view's units, as a page with no file states them.
    let so_far = octocode_engine::types::TextSearchStats {
        match_count: (local_search_query::view(query) != local_search_query::View::CountLines)
            .then(|| saturating(searched.matches())),
        matched_lines: Some(saturating(searched.matched_lines())),
        files_searched: Some(saturating(searched.files_searched())),
        ..octocode_engine::types::TextSearchStats::default()
    };
    STANDINGS.put(
        key(query_key, position, &pin),
        policy.to_owned(),
        searched.weight(),
        Mutex::new(Some(searched)),
    );
    Ok(LocalSearchResult {
        status: SearchStatus::Partial,
        stats: SearchStats {
            partial_reasons: vec![PartialReason::ScanBudget],
            // A paused scan has not counted every file yet.
            matched_files: None,
            ..super::executor::search_stats(local_search_query::view(query), &so_far, 0)
        },
        unique_values: None,
        files: Vec::new(),
        pagination: None,
        selection: None,
        hints: Vec::new(),
        next: Some(octocode_runtime_base::response::collection::walk_calls(
            octocode_runtime_base::id::ToolId::LocalSearch,
            None,
            Continuation::continued_query(resume),
        )),
        is_partial: true,
        terminal_limit: false,
        warnings: vec![format!(
            "The PCRE2 (regex:\"pcre2\") search paused at its time budget after {position} files in walk order (matchCount and filesScanned count those): the result ranks every file in scope, so files are listed once the search ends; `next.nextPage` resumes it there."
        )],
        skipped_paths: None,
        source_snapshot: None,
    })
}

#[cfg(test)]
thread_local! {
    static TEST_BUDGET: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// The time a test window's clock advances at each file it finishes: far
/// past any real run, so only the injected clock ends a test window.
#[cfg(test)]
const TEST_STEP: std::time::Duration = std::time::Duration::from_secs(3600);

/// A test window: it ends once `files` files past where it resumed are
/// finished, on every run and machine.
#[cfg(test)]
fn test_window() -> Option<SearchWindow> {
    let files = TEST_BUDGET.with(std::cell::Cell::get)?;
    let started = std::time::Instant::now();
    let read = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    Some(SearchWindow {
        ends: Some(started + TEST_STEP * files),
        clock: Some(std::sync::Arc::new(move || {
            started + TEST_STEP * (read.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1)
        })),
        resume: None,
    })
}

/// PCRE2 first pages on this thread end their window after `files` files
/// past where they resume, until the guard drops (`None`: they run whole).
#[cfg(test)]
pub(super) fn test_budget(files: Option<u32>) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_BUDGET.with(|budget| budget.set(None));
        }
    }
    TEST_BUDGET.with(|budget| budget.set(files));
    Reset
}

/// Drop every window kept for searches of the workspace `root`, as eviction
/// or another process would; what other tests in the process kept stays.
#[cfg(test)]
pub(super) fn forget_kept(root: &std::path::Path) {
    STANDINGS.evict_policy(&octocode_runtime_base::test_support::workspace_policy(root).identity());
}
