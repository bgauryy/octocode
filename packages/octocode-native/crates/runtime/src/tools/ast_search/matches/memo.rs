use super::MatchQuery;
use super::scan::{scan_directory, scan_file, sort_files};
use super::snapshot::memo_source_unchanged;
use super::types::{Scan, Scope};
use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use octocode_engine::structural::{FileMatches, StructuralDiagnostic};
use serde_json::json;

/// Scans kept for the continuation pages of a multi-page result.
pub(super) static SCANS: super::super::memo::ScanMemo<Scan> = super::super::memo::ScanMemo::new();

/// The scan inputs a `match` snapshot does not digest; a stored scan is
/// served only to a page with the same ones. The row shape is one of them:
/// a priced scan's prices are the sizes of rows of that shape.
pub(super) fn scan_key(q: MatchQuery<'_>, paths: &PathPolicy) -> String {
    crate::digest::json_sha256(&json!([
        paths.identity(),
        q.default_excludes(),
        super::super::source::MAX_PARSE_SOURCE_BYTES,
        q.capture_text().unwrap_or(false),
        q.provenance(),
        q.test_code()
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

/// A fresh scan of the row's scope and when it started. Every request owns
/// its scan: identical concurrent requests do not share one.
pub(super) fn fresh_scan(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(std::sync::Arc<Scan>, std::time::SystemTime), ToolError> {
    let started = octocode_engine::stamp::settle_clock();
    let mut scan = if scope.is_file {
        scan_file(q, scope, paths, cancel)?
    } else {
        scan_directory(q, scope, paths, cancel)?
    };
    cancel.check().map_err(ToolError::cancelled)?;
    sort_files(q, &mut scan.files);
    Ok((std::sync::Arc::new(scan), started))
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
