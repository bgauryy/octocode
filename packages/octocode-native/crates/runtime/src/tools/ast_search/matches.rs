use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use exact_miss::{exact_miss, kind_broaden};
use gaps::ScopeGaps;
use groups::{group_files, rebuild_page};
use memo::{SCANS, remember_scan, scan_key, shared_scan, stored_scan};
use octocode_engine::structural::{StructuralDemand, StructuralDetailedMatch};
use qualified::qualified_miss;
use render::{file_page, render_match_page};
use rows::{MatchRow, diag, match_value};
use scope::{Inapplicable, compile_in_grammars, infer_grammar, resolve_scope};
use serde_json::Value;
use snapshot::{interrupted_rescan, match_snapshot};

/// The views that list files, not matches.
fn file_list(q: MatchQuery<'_>) -> bool {
    matches!(q.result_view().as_deref(), Some("files" | "countMatches"))
}

/// The shown length of a match value (the core match-text budget).
fn content_length(q: MatchQuery<'_>) -> usize {
    if q.capture_text().unwrap_or(false) {
        crate::id::ast_search_policy::MAX_MATCH_CONTENT_LENGTH
    } else {
        crate::id::ast_search_policy::MATCH_CONTENT_LENGTH
    }
}

/// The size a match's row adds to its match page.
fn row_chars(row: &MatchRow) -> usize {
    octocode_engine::json_size::utf16_units(&row.value).unwrap_or(0) + 1
}

/// A match's row under `q`'s row shape.
fn match_row(
    q: MatchQuery<'_>,
    m: &StructuralDetailedMatch,
    cancel: &dyn CancellationCheck,
) -> Result<MatchRow, ToolError> {
    match_value(
        m,
        q.capture_text().unwrap_or(false),
        content_length(q),
        q.provenance(),
        cancel,
    )
}

/// The row price of a match under `q`'s row shape.
fn match_price(
    q: MatchQuery<'_>,
    m: &StructuralDetailedMatch,
    cancel: &dyn CancellationCheck,
) -> Result<u32, ToolError> {
    Ok(u32_of_usize(row_chars(&match_row(q, m, cancel)?)))
}

/// What a directory scan keeps of each match: a file view needs only the
/// counts; a match view prices every row (pages are cut from the prices)
/// and rebuilds only the rows of the file page it shows.
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
    run_match(q, paths, cancel, response_window).map(super::output::finish)
}

fn run_match(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let scope = resolve_scope(q, paths, cancel)?;
    let continues = q.page() > 1 || q.match_page() > 1;
    let key = scan_key(q, paths);
    // A continuation page reuses the scan its first page stored when nothing
    // it read changed; the snapshot below still decides whether it serves.
    let stored = q
        .snapshot()
        .filter(|_| continues)
        .and_then(|snapshot| stored_scan(&snapshot, &key, &scope));
    let fresh = stored.is_none();
    let (scan, started) = match stored {
        Some(scan) => (scan, octocode_engine::stamp::settle_clock()),
        None => shared_scan(q, &scope, paths, cancel, &key)?,
    };
    let mut scope = scope;
    let grammars = compile_in_grammars(q, &scope, &scan);
    scope.inferred = infer_grammar(q, &scope, &scan, &grammars)?;
    let inapplicable = Inapplicable::of(&grammars);
    let snapshot = match_snapshot(q, &scope, &scan);
    // A page number without a snapshot asks for that page of a fresh scan;
    // only a snapshot that no longer matches is stale.
    if continues && q.snapshot().is_some_and(|given| given != snapshot) {
        if !fresh {
            // A stored scan is only a candidate: a page whose query no longer
            // matches it is answered by a fresh scan, exactly as without one.
            if let Some(stale) = q.snapshot() {
                SCANS.evict(&stale);
            }
            return execute_match(q, paths, cancel, response_window);
        }
        if let Some(error) = interrupted_rescan(q, &scan) {
            return Err(error);
        }
        return Ok(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
        ));
    }
    let budget = crate::response::page_windows::page_budget(response_window, &q.to_value());
    let row_paths = super::RowPaths::new(paths, &scope.canonical, scope.is_file);
    let mut grouped = group_files(q, &scan.files, budget, row_paths, &inapplicable, cancel)?;
    grouped.skipped_grammars = inapplicable.counts();
    // Group paths are rooted at the scope's parent; the notice names them as
    // the response names rows.
    grouped.recovered = grouped
        .recovered
        .respelled(|shown| row_paths.delivered(shown));
    let shown = file_page(q, &grouped.groups, budget).window;
    match rebuild_page(
        q,
        &scope,
        &scan.files,
        &mut grouped.groups[shown],
        budget,
        row_paths,
        cancel,
    ) {
        Ok(incomplete) => {
            // Counted and disclosed as the scan counts a file its limit cut.
            grouped.unevaluated_files += incomplete.len() as u32;
            grouped
                .diagnostics
                .extend(incomplete.iter().flatten().map(diag));
        }
        Err(groups::RebuildError::Evidence(error)) => return Err(error),
        Err(groups::RebuildError::SourceChanged) => {
            // A file on this page changed after the scan priced it.
            if !fresh {
                if let Some(stale) = q.snapshot() {
                    SCANS.evict(&stale);
                }
                return execute_match(q, paths, cancel, response_window);
            }
            return Ok(crate::response::result::stale_snapshot(
                crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
            ));
        }
    }
    if !scan.truncated
        && q.scan_offset() == 0
        && !grammars.is_empty()
        && grammars.iter().all(|grammar| grammar.outcome.is_err())
        && let Some(Err(error)) = grammars.first().map(|grammar| grammar.outcome.clone())
    {
        // Every grammar the whole scope holds (even in files the literal
        // prefilter left unparsed) rejects the query: only an initial
        // whole-scope scan can reject it for the selected set.
        return Err(ToolError::new(error.code, error.message));
    }
    if grouped.total_matches == 0
        && grouped.parsed_files == 0
        && grouped.compile_failed_files > 0
        && !scan.truncated
        && q.scan_offset() == 0
        && let Some(error) = grouped.compile_error
    {
        // Only a whole-scope scan proves there is no compatible candidate.
        // A capped incompatible window must retain coverage and scan onward.
        return Err(error);
    }
    let mut gaps = if scope.is_file {
        ScopeGaps::default()
    } else if grouped.total_matches == 0 && grouped.parsed_files == 0 && q.language().is_some() {
        ScopeGaps::of(q, &scope, paths, cancel)
    } else {
        // Files of the grammar behind skipped paths are still a reason,
        // beside the pattern's shape or the matches found: whether or not
        // the scan parsed any, and whether one language or each file's own
        // grammar selected them.
        ScopeGaps::beside_parsed(q, &scope, paths, cancel)
    };
    // The skipped-path probe walks within the scan's own file bound: a scan
    // cut at that bound cuts the probe too, and the scan's cut (with its
    // continuation) already says the result is partial.
    if scan.truncated {
        gaps.skipped = gaps.skipped.filter(|skipped| !skipped.disclosure.unchecked);
    }
    let qualified = qualified_miss(q, &scope, &scan, (paths, row_paths), cancel);
    let exact_miss_scope = qualified.is_none()
        && grouped.total_matches == 0
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
        (budget, row_paths),
        cancel,
    )?;
    if let Some(qualified) = qualified {
        qualified.attach(&mut out);
    }
    if exact_miss_scope {
        match exact_miss(q, &scope, paths, cancel) {
            Some(miss) => miss.attach(&mut out),
            None => {
                if let Some(lead) = kind_broaden(q, &scope, paths, cancel) {
                    out["calls"]["broadenSearch"] = lead;
                }
            }
        }
    }
    if fresh
        && !scan.untracked_ignore_dependencies
        && out["calls"]
            .as_object()
            .is_some_and(|next| next.contains_key("nextPage") || next.contains_key("nextMatchPage"))
    {
        remember_scan((snapshot, key), &scope, &scan, started);
    }
    Ok(out)
}

mod clipped;
mod continuations;
mod exact_miss;
mod gaps;
mod groups;
mod memo;
mod qualified;
mod query;
mod render;
mod rows;
mod scan;
mod scope;
mod snapshot;
mod types;
pub use query::MatchQuery;

#[cfg(test)]
mod bounded_text_tests;
#[cfg(test)]
mod interrupted_rescan_tests;
#[cfg(test)]
mod language_glob_tests;
#[cfg(test)]
mod memo_tests;

#[cfg(test)]
mod source_freshness_tests;

#[cfg(test)]
mod discovery_freshness_tests;

#[cfg(test)]
mod external_ignore_freshness_tests;

#[cfg(test)]
mod flight_tests;

#[cfg(test)]
mod demand_tests;
