use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use exact_miss::exact_miss;
use gaps::ScopeGaps;
use groups::{fill_page, group_files};
use memo::{SCANS, fresh_scan, remember_scan, scan_key, stored_scan};
use octocode_engine::num::saturating_u32;
use octocode_engine::structural::{StructuralDemand, StructuralDetailedMatch};
use qualified::qualified_miss;
use query::OwnedMatch;
use render::render_match_page;
use rows::{MatchRow, diag, match_value};
use scan::sort_files;
use scope::{Inapplicable, compile_in_grammars, infer_grammar, resolve_scope};
use serde_json::Value;
use snapshot::{interrupted_rescan, match_snapshot_under};

/// The views that list files, not matches.
fn file_list(q: MatchQuery<'_>) -> bool {
    matches!(q.result_view().as_deref(), Some("files" | "countMatches"))
}

/// The size a match's row adds to its page.
fn row_chars(row: &MatchRow) -> usize {
    octocode_engine::json_size::utf16_units(&row.value).unwrap_or(0) + 1
}

/// A match's row under `q`'s row shape.
fn match_row(
    q: MatchQuery<'_>,
    m: &StructuralDetailedMatch,
    cancel: &dyn CancellationCheck,
) -> Result<MatchRow, ToolError> {
    match_value(m, q.capture_text().unwrap_or(false), q.provenance(), cancel)
}

/// The row price of a match under `q`'s row shape.
fn match_price(
    q: MatchQuery<'_>,
    m: &StructuralDetailedMatch,
    cancel: &dyn CancellationCheck,
) -> Result<u32, ToolError> {
    Ok(saturating_u32(row_chars(&match_row(q, m, cancel)?)))
}

/// What a directory scan keeps of each match: a file view needs only the
/// counts; a match view prices every row (pages are cut from the prices)
/// and rebuilds only the rows of the page it shows.
fn demand<'a>(
    q: MatchQuery<'_>,
    price: &'a (dyn Fn(&StructuralDetailedMatch) -> u32 + Sync),
) -> StructuralDemand<'a> {
    if file_list(q) {
        StructuralDemand::Count
    } else {
        StructuralDemand::Priced(price)
    }
}

pub fn execute_match(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    run_match_under(
        crate::build_identity::BUILD_IDENTITY,
        q,
        paths,
        cancel,
        response_window,
    )
    .map(crate::response::result::finished)
}

/// A window past the first whose candidates before it are not the ones the
/// windows before it evaluated: its `scanOffset` counts other files.
fn window_moved(q: MatchQuery<'_>, scan: &types::Scan) -> bool {
    q.snapshot()
        .is_some_and(|pin| q.scan_offset() > 0 && scan.listed_before.as_ref() != Some(&pin))
}

/// What a fresh scan of a row's window answers.
enum Fresh {
    /// The answer itself: a moved window's restart, or a ranked window the
    /// time budget paused ([`standing`]).
    Answered(Value),
    /// The scan, sorted, and when it started; with a ranked scan's last
    /// window, the query its pages answer as (its first window's).
    Scanned(
        Option<Box<OwnedMatch>>,
        Box<types::Scan>,
        std::time::SystemTime,
    ),
}

/// A fresh scan of `q`'s window: a ranked order's windows add up to the whole
/// scope before any file is listed, and its later pages scan the whole scope.
fn fresh_window(
    q: MatchQuery<'_>,
    scope: &types::Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    continues: bool,
) -> Result<Fresh, ToolError> {
    let ranked = !scope.is_file && standing::ranks_whole_scope(q);
    let (mut scan, mut started) = if continues && ranked {
        standing::rescan(q, scope, paths, cancel)?
    } else {
        fresh_scan(q, scope, paths, cancel)?
    };
    let moved = || {
        Fresh::Answered(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
        ))
    };
    if !continues && window_moved(q, &scan) {
        return Ok(moved());
    }
    let mut from_start = None;
    if !continues && ranked {
        match standing::advance(q, scope, paths, cancel, (scan, started))? {
            standing::Ranked::Paused(page) => return Ok(Fresh::Answered(page)),
            standing::Ranked::Moved => return Ok(moved()),
            standing::Ranked::Whole(whole, first_started) => {
                (scan, started) = (*whole, first_started);
                from_start = Some(Box::new(q.at_scan_start()));
            }
        }
    }
    sort_files(
        from_start.as_deref().map_or(q, OwnedMatch::query),
        &mut scan.files,
    );
    Ok(Fresh::Scanned(from_start, Box::new(scan), started))
}

/// [`execute_match`] as the build `build` would answer: the build is an input
/// of every snapshot, so a page minted by another build is refused.
fn run_match_under(
    build: &str,
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let scope = resolve_scope(q, paths, cancel)?;
    let continues = q.page() > 1;
    let key = scan_key(q, paths);
    // A continuation page reuses the scan its first page stored when nothing
    // it read changed; the snapshot below still decides whether it serves.
    let stored = q
        .snapshot()
        .filter(|_| continues)
        .and_then(|snapshot| stored_scan(&snapshot, &key, &scope));
    let fresh = stored.is_none();
    // The query a ranked scan's last window answers as: its first window's.
    let from_start;
    let (q, scan, started) = match stored {
        Some(scan) => (q, scan, octocode_engine::stamp::settle_clock()),
        None => match fresh_window(q, &scope, paths, cancel, continues)? {
            Fresh::Answered(out) => return Ok(out),
            Fresh::Scanned(whole, scan, started) => {
                from_start = whole;
                let q = from_start.as_deref().map_or(q, OwnedMatch::query);
                (q, std::sync::Arc::from(scan), started)
            }
        },
    };
    let mut scope = scope;
    let grammars = compile_in_grammars(q, &scope, &scan);
    scope.inferred = infer_grammar(q, &scope, &scan, &grammars, paths, cancel)?;
    // The parsers the walk read files with, in a grammar the query compiles in.
    scope.compiled = scan
        .census
        .keys()
        .filter(|extension| {
            grammars
                .iter()
                .any(|grammar| grammar.outcome.is_ok() && grammar.extensions.contains(*extension))
        })
        .cloned()
        .collect();
    let inapplicable = Inapplicable::of(q, &scan);
    let snapshot = match_snapshot_under(build, q, &scope, &scan);
    // A page number without a snapshot asks for that page of a fresh scan;
    // only a snapshot that no longer matches is stale.
    if continues && q.snapshot().is_some_and(|given| given != snapshot) {
        if !fresh {
            // A stored scan is only a candidate: a page whose query no longer
            // matches it is answered by a fresh scan, exactly as without one.
            if let Some(stale) = q.snapshot() {
                SCANS.evict(&stale);
            }
            return run_match_under(build, q, paths, cancel, response_window)
                .map(crate::response::result::finished);
        }
        if let Some(error) = interrupted_rescan(q, &scan) {
            return Err(error);
        }
        return Ok(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
        ));
    }
    let budget = crate::response::page_windows::page_budget(response_window);
    let row_paths = super::RowPaths::new(paths, &scope.canonical, scope.is_file);
    let mut grouped = group_files(q, &scan.files, row_paths, cancel)?;
    grouped.skipped_grammars = inapplicable.counts();
    // Group paths are rooted at the scope's parent; the notice names them as
    // the response names rows.
    grouped.recovered = grouped
        .recovered
        .respelled(|shown| row_paths.delivered(shown));
    grouped.recovered_bodies = grouped
        .recovered_bodies
        .respelled(|shown| row_paths.delivered(shown));
    let page = stream::stream_page(q, &grouped.groups, budget);
    let rows = page_rows::PageRows {
        snapshot: &snapshot,
        key: &key,
        proved: !fresh,
    };
    let shown = (&mut grouped, &page);
    match show_page(q, (&scope, &scan), shown, row_paths, rows, cancel) {
        Ok(()) => {}
        Err(groups::RebuildError::Evidence(error)) => return Err(error),
        Err(groups::RebuildError::SourceChanged) => {
            // A file on this page changed after the scan priced it.
            if !fresh {
                if let Some(stale) = q.snapshot() {
                    SCANS.evict(&stale);
                }
                return run_match_under(build, q, paths, cancel, response_window)
                    .map(crate::response::result::finished);
            }
            return Ok(crate::response::result::stale_snapshot(
                crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
            ));
        }
    }
    if !scan.truncated
        && q.scan_offset() == 0
        && let Some(error) = inapplicable.rejection()
    {
        // No file of the whole scope parses the query (even one the literal
        // prefilter would leave unread): only an initial whole-scope scan can
        // say so. A window cut short counts what it met and scans onward.
        return Err(ToolError::new(error.code, error.message.clone()));
    }
    // A window with no rows and no continuation states absence, so it names the
    // skipped walks though the scan's first page already did.
    let states_absence = grouped.groups.is_empty() && !scan.truncated;
    let gaps = if scope.is_file {
        ScopeGaps::default()
    } else if grouped.total_matches == 0 && grouped.parsed_files == 0 && q.language().is_some() {
        ScopeGaps::of(q, &scope, &scan, paths, states_absence, cancel)
    } else {
        // Files of the grammar behind skipped paths are still a reason,
        // beside the pattern's shape or the matches found: whether or not
        // the scan parsed any, and whether one language or each file's own
        // grammar selected them.
        ScopeGaps::beside_parsed(q, &scope, paths, states_absence, cancel)
    };
    let qualified = qualified_miss(q, &scope, &scan, (paths, row_paths), cancel);
    let exact_miss_scope = grouped.total_matches == 0
        && grouped.scanned_files > 0
        && !scan.truncated
        && q.scan_offset() == 0;
    let mut out = render_match_page(
        q,
        &scope,
        &scan,
        grouped,
        &snapshot,
        gaps,
        (&page, row_paths),
        cancel,
    )?;
    if let Some(hint) = exact_miss::fuller_form_hint(q, &scope) {
        crate::response::result::add_hints(&mut out, [hint]);
    }
    if exact_miss_scope {
        explain_miss(q, (&scope, &*scan), paths, cancel, &mut out);
    }
    if let Some(qualified) = qualified {
        qualified.attach(&mut out);
    }
    if fresh
        && out["calls"]
            .as_object()
            .is_some_and(|next| next.contains_key("nextPage"))
    {
        remember_scan((snapshot, key), &scope, &scan, paths, started);
    }
    Ok(out)
}

/// Fill the rows `page` shows of `grouped`'s files. A shown file whose match did not finish is counted and
/// disclosed as the scan counts a file its limit cut.
fn show_page(
    q: MatchQuery<'_>,
    (scope, scan): (&types::Scope, &types::Scan),
    (grouped, page): (&mut types::Grouped, &stream::StreamPage),
    row_paths: super::RowPaths<'_>,
    rows: page_rows::PageRows<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<(), groups::RebuildError> {
    let shown = &mut grouped.groups[page.window.clone()];
    let incomplete = fill_page(
        q,
        scope,
        &scan.files,
        (shown, &page.slices),
        row_paths,
        Some(rows),
        cancel,
    )?;
    grouped.unevaluated_files = grouped
        .unevaluated_files
        .saturating_add(saturating_u32(incomplete.len()));
    grouped
        .diagnostics
        .extend(incomplete.iter().flatten().map(diag));
    Ok(())
}

/// What an empty whole scan can say of its rule: the literal tokens of its
/// patterns that look like metavariables (a note beside the rest, never in
/// place of it), then the first that applies of: its relation read only a
/// neighbor, its text is another kind's, or its pattern differs from the
/// declarations the scope holds.
fn explain_miss(
    q: MatchQuery<'_>,
    (scope, scan): (&types::Scope, &types::Scan),
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) {
    literal_miss::attach_literal_miss(q, scope, scan, cancel, out);
    if relation_miss::attach_relation_miss(q, scope, paths, cancel, out)
        || kind_miss::attach_kind_miss(q, scope, paths, cancel, out)
    {
        return;
    }
    match exact_miss(q, scope, paths, cancel) {
        Some(miss) => miss.attach(out),
        None => {
            construct_miss::attach_construct_miss(q, scope, paths, cancel, out);
        }
    }
}

mod construct_miss;
mod continuations;
mod exact_miss;
mod gaps;
mod groups;
mod kind_miss;
mod literal_miss;
mod memo;
mod page_rows;
mod qualified;
mod query;
mod reads;
mod relation_miss;
mod render;
mod rows;
mod scan;
mod scope;
mod snapshot;
mod standing;
mod stream;
mod types;
pub use query::MatchQuery;

#[cfg(test)]
mod interrupted_rescan_tests;
#[cfg(test)]
mod language_glob_tests;
#[cfg(test)]
mod literal_miss_tests;
#[cfg(test)]
mod memo_tests;

#[cfg(test)]
mod source_freshness_tests;

#[cfg(test)]
mod discovery_freshness_tests;

#[cfg(test)]
mod external_ignore_freshness_tests;

#[cfg(test)]
mod concurrent_scan_tests;

#[cfg(test)]
mod build_identity_tests;
#[cfg(test)]
mod demand_tests;

#[cfg(test)]
mod page_rows_tests;

#[cfg(test)]
mod unsaid_tests;

#[cfg(test)]
mod standing_tests;
