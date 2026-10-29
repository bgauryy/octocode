use super::scan::{scan_directory, scan_file, sort_files};
use super::snapshot::memo_source_unchanged;
use super::types::{Scan, Scope};
use super::{MatchQuery, content_length};
use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use octocode_engine::structural::{FileMatches, StructuralDiagnostic};
use serde_json::json;

/// Scans kept for the continuation pages of a multi-page result.
pub(super) static SCANS: super::super::memo::ScanMemo<Scan> = super::super::memo::ScanMemo::new();

/// One scan per distinct query at a time: identical rows of one batch wait
/// for the first one's scan and its start time (the memo's settle check).
pub(super) static SCAN_FLIGHT: octocode_io::flight::KeyedFlight<
    String,
    Option<(std::sync::Arc<Scan>, std::time::SystemTime)>,
> = octocode_io::flight::KeyedFlight::transient();

/// The flight a row joins: every query field but its page cursors (rows that
/// differ only in the page they show scan alike), plus the scan inputs the
/// query does not name.
pub(super) fn flight_key(q: MatchQuery<'_>, scan_key: &str) -> String {
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
pub(super) fn scan_key(q: MatchQuery<'_>, paths: &PathPolicy) -> String {
    crate::digest::json_sha256(&json!([
        paths.identity(),
        q.default_excludes(),
        super::super::source::MAX_PARSE_SOURCE_BYTES,
        q.capture_text().unwrap_or(false),
        content_length(q),
        q.provenance()
    ]))
}

/// Rough heap bytes of a stored scan, its memo weight.
fn scan_bytes(scan: &Scan) -> usize {
    let mut templates = std::collections::HashSet::new();
    let mut catalogs = std::collections::HashSet::new();
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
                        let template_bytes =
                            if templates.insert(std::sync::Arc::as_ptr(&m.source_templates)) {
                                m.source_templates.heap_bytes()
                            } else {
                                0
                            };
                        let catalog_bytes = m.source_descriptors.as_ref().map_or(0, |catalog| {
                            if catalogs.insert(std::sync::Arc::as_ptr(catalog)) {
                                catalog.owned_bytes()
                            } else {
                                0
                            }
                        });
                        template_bytes
                            + catalog_bytes
                            + m.source_origin
                                .as_ref()
                                .map_or(0, octocode_engine::text::analysis::OriginProjection::owned_bytes)
                            + m.header_origin
                                .as_ref()
                                .map_or(0, octocode_engine::text::analysis::OriginProjection::owned_bytes)
                            + 128
                            + m.text.len()
                            + m.header.as_ref().map_or(0, String::len)
                            + m.metavars
                                .values()
                                .flatten()
                                .map(|value| 24 + value.len())
                                .sum::<usize>()
                            + m.metavar_ranges
                                .values()
                                .flatten()
                                .map(|range| {
                                    48 + range.text.len()
                                        + range
                                            .source_origin
                                            .as_ref()
                                            .map_or(0, octocode_engine::text::analysis::OriginProjection::owned_bytes)
                                })
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

/// The scan stored for `snapshot` under `key`, while nothing it read
/// changed; a changed one leaves the memo.
pub(super) fn stored_scan(
    snapshot: &str,
    key: &str,
    scope: &Scope,
) -> Option<std::sync::Arc<Scan>> {
    let scan = SCANS.get(snapshot, key)?;
    if !scan.untracked_ignore_dependencies && memo_source_unchanged(scope, &scan) {
        Some(scan)
    } else {
        SCANS.evict(snapshot);
        None
    }
}

/// A fresh scan of the row's scope and when it started. Identical rows
/// running at once (one batch) share one scan: the first scans, the others
/// take its scan from the flight's slot.
pub(super) fn shared_scan(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    key: &str,
) -> Result<(std::sync::Arc<Scan>, std::time::SystemTime), ToolError> {
    let ticket = SCAN_FLIGHT.join(flight_key(q, key));
    // Rows run on blocking workers, never on an async one. A waiter
    // stops on its own cancellation while another request scans.
    let mut slot = ticket
        .blocking_turn_until(|| cancel.check())
        .map_err(ToolError::cancelled)?;
    if let Some((scan, started)) = slot.as_ref() {
        return Ok((std::sync::Arc::clone(scan), *started));
    }
    let started = octocode_engine::stamp::settle_clock();
    let mut scan = if scope.is_file {
        scan_file(q, scope, cancel)?
    } else {
        scan_directory(q, scope, paths, cancel)?
    };
    cancel.check().map_err(ToolError::cancelled)?;
    sort_files(q, &mut scan.files);
    let scan = std::sync::Arc::new(scan);
    // Only a scan that completed is shared; a failed one
    // leaves the next row to scan itself.
    *slot = Some((std::sync::Arc::clone(&scan), started));
    Ok((scan, started))
}

/// Keep a fresh scan for the continuation pages of its result, watched for
/// changes to every source it read.
pub(super) fn remember_scan(
    (snapshot, key): (String, String),
    scope: &Scope,
    scan: &std::sync::Arc<Scan>,
    started: std::time::SystemTime,
) {
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
        std::sync::Arc::clone(scan),
        scan_bytes(scan),
    );
}
