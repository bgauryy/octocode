use crate::contracts::query_limits::ast_search::r#match as limits;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use gaps::ScopeGaps;
use groups::{group_files, rebuild_page};
use octocode_engine::structural::{
    FileMatches, StructuralDemand, StructuralDetailedMatch, StructuralDiagnostic,
};
use render::{file_page, render_match_page};
use rows::{MatchRow, diag, match_value};
use scan::{scan_directory, scan_file, sort_files};
use scope::{Inapplicable, compile_in_grammars, exact_miss, infer_grammar, resolve_scope};
use serde_json::json;
use snapshot::{interrupted_rescan, match_snapshot, memo_source_unchanged};
use types::Scan;

/// Scans kept for the continuation pages of a multi-page result.
static SCANS: super::memo::ScanMemo<Scan> = super::memo::ScanMemo::new();

/// One scan per distinct query at a time: identical rows of one batch wait
/// for the first one's scan and its start time (the memo's settle check).
static SCAN_FLIGHT: octocode_io::flight::KeyedFlight<
    String,
    Option<(std::sync::Arc<Scan>, std::time::SystemTime)>,
> = octocode_io::flight::KeyedFlight::transient();

/// The flight a row joins: every query field but its page cursors (rows that
/// differ only in the page they show scan alike), plus the scan inputs the
/// query does not name.
fn flight_key(q: MatchQuery<'_>, scan_key: &str) -> String {
    let mut row = q.to_value();
    if let Some(fields) = row.as_object_mut() {
        for field in ["page", "matchPage", "snapshot"] {
            fields.shift_remove(field);
        }
    }
    crate::digest::json_sha256(&json!([row, scan_key]))
}

/// The scan inputs a `match` snapshot does not digest; a stored scan is
/// served only to a page with the same ones. The row shape is one of them:
/// a priced scan's prices are the sizes of rows of that shape.
fn scan_key(q: MatchQuery<'_>, paths: &PathPolicy) -> String {
    crate::digest::json_sha256(&json!([
        paths.identity(),
        q.default_excludes(),
        super::source::MAX_PARSE_SOURCE_BYTES,
        q.capture_text().unwrap_or(false),
        content_length(q)
    ]))
}

/// The views that list files, not matches.
fn file_list(q: MatchQuery<'_>) -> bool {
    matches!(q.result_view().as_deref(), Some("files" | "countMatches"))
}

/// The shown length of a match value (`matchContentLength`).
fn content_length(q: MatchQuery<'_>) -> usize {
    q.match_content_length()
        .map_or(limits::MATCH_CONTENT_LENGTH_DEFAULT, |length| {
            length as usize
        })
        .clamp(1, limits::MATCH_CONTENT_LENGTH_MAXIMUM)
}

/// The size a match's row adds to its match page.
fn row_chars(row: &MatchRow) -> usize {
    octocode_engine::json_size::utf16_units(&row.value).unwrap_or(0) + 1
}

/// The row price of a match under `q`'s row shape.
fn match_price(q: MatchQuery<'_>, m: &StructuralDetailedMatch) -> u32 {
    let row = match_value(m, q.capture_text().unwrap_or(false), content_length(q));
    u32_of_usize(row_chars(&row))
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

/// Rough heap bytes of a stored scan, its memo weight.
fn scan_bytes(scan: &Scan) -> usize {
    let diagnostic =
        |d: &StructuralDiagnostic| 64 + d.message.len() + d.path.as_ref().map_or(0, String::len);
    scan.files
        .iter()
        .map(|file| {
            96 + file.path.len()
                + file.source.as_ref().map_or(0, |(path, digest)| {
                    64 + path.as_os_str().len() + digest.len()
                })
                + match &file.matches {
                    FileMatches::Priced(prices) => prices.len() * 4,
                    FileMatches::Held(_) | FileMatches::Counted(_) => 0,
                }
                + file
                    .matches
                    .held()
                    .iter()
                    .map(|m| {
                        128 + m.text.len()
                            + m.header.as_ref().map_or(0, String::len)
                            + m.metavars
                                .values()
                                .flatten()
                                .map(|value| 24 + value.len())
                                .sum::<usize>()
                            + m.metavar_ranges
                                .values()
                                .flatten()
                                .map(|range| 48 + range.text.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
                + file.diagnostics.iter().map(diagnostic).sum::<usize>()
        })
        .sum::<usize>()
        + scan.diagnostics.iter().map(diagnostic).sum::<usize>()
        + scan.source_digest.len()
        + scan
            .sources
            .iter()
            .chain(&scan.discovery_sources)
            .map(|path| 64 + path.as_os_str().len())
            .sum::<usize>()
}

pub fn execute_match(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> super::AstResult {
    cancel.check().map_err(ToolError::cancelled)?;
    let scope = resolve_scope(q, paths, cancel)?;
    let continues = q.page() > 1 || q.match_page() > 1;
    let key = scan_key(q, paths);
    // A continuation page reuses the scan its first page stored when nothing
    // it read changed; the snapshot below still decides whether it serves.
    let stored = q
        .snapshot()
        .filter(|_| continues)
        .and_then(|snapshot| SCANS.get(&snapshot, &key))
        .filter(|scan| {
            if !scan.untracked_ignore_dependencies && memo_source_unchanged(&scope, scan) {
                true
            } else {
                if let Some(snapshot) = q.snapshot() {
                    SCANS.evict(&snapshot);
                }
                false
            }
        });
    let fresh = stored.is_none();
    let (scan, started) = match stored {
        Some(scan) => (scan, octocode_engine::stamp::settle_clock()),
        None => {
            // Identical rows running at once (one batch) share one scan: the
            // first scans, the others take its scan from the flight's slot.
            let ticket = SCAN_FLIGHT.join(flight_key(q, &key));
            // Rows run on blocking workers, never on an async one. A waiter
            // stops on its own cancellation while another request scans.
            let mut slot = ticket
                .blocking_turn_until(|| cancel.check())
                .map_err(ToolError::cancelled)?;
            match slot.as_ref() {
                Some((scan, started)) => (std::sync::Arc::clone(scan), *started),
                None => {
                    let started = octocode_engine::stamp::settle_clock();
                    let mut scan = if scope.is_file {
                        scan_file(q, &scope)?
                    } else {
                        scan_directory(q, &scope, paths, cancel)?
                    };
                    cancel.check().map_err(ToolError::cancelled)?;
                    sort_files(q, &mut scan.files);
                    let scan = std::sync::Arc::new(scan);
                    // Only a scan that completed is shared; a failed one
                    // leaves the next row to scan itself.
                    *slot = Some((std::sync::Arc::clone(&scan), started));
                    (scan, started)
                }
            }
        }
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
    let mut grouped = group_files(q, &scan.files, budget, row_paths, &inapplicable);
    grouped.inapplicable_notice = inapplicable.notice();
    // Group paths are rooted at the scope's parent; the notice names them as
    // the response names rows.
    grouped.recovered = grouped
        .recovered
        .respelled(|shown| row_paths.delivered(shown));
    let shown = file_page(q, &grouped.groups, budget).window;
    if let Ok(incomplete) = rebuild_page(
        q,
        &scope,
        &scan.files,
        &mut grouped.groups[shown],
        budget,
        row_paths,
    ) {
        // Counted and disclosed as the scan counts a file its limit cut.
        grouped.unevaluated_files += incomplete.len() as u32;
        grouped
            .diagnostics
            .extend(incomplete.iter().flatten().map(diag));
    } else {
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
    let exact_miss_scope = grouped.total_matches == 0
        && grouped.scanned_files > 0
        && !scan.truncated
        && q.scan_offset() == 0;
    let mut out = render_match_page(q, &scope, &scan, grouped, &snapshot, gaps, budget)?;
    if exact_miss_scope && let Some(miss) = exact_miss(q, &scope, paths, cancel) {
        miss.attach(&mut out);
    }
    if fresh
        && !scan.untracked_ignore_dependencies
        && out["next"]
            .as_object()
            .is_some_and(|next| next.contains_key("nextPage") || next.contains_key("nextMatchPage"))
    {
        let watched = scan
            .sources
            .iter()
            .chain(&scan.discovery_sources)
            .cloned()
            .collect::<Vec<_>>();
        SCANS.put(
            (snapshot, key),
            crate::tools::page_memo::Watch {
                root: &scope.canonical,
                sources: &watched,
                rules: &scan.ignore_rules,
                markers: &scan.repository_markers,
            },
            started,
            std::sync::Arc::clone(&scan),
            scan_bytes(&scan),
        );
    }
    Ok(out)
}

mod clipped;
mod continuations;
mod gaps;
mod groups;
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
