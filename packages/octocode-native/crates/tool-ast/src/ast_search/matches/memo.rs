use super::MatchQuery;
use super::scan::{scan_directory, scan_file};
use super::snapshot::memo_source_unchanged;
use super::types::{Scan, Scope};
use octocode_engine::structural::{FileMatches, StructuralDiagnostic};
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use serde_json::json;

/// Scans kept for the continuation pages of a multi-page result.
pub(super) static SCANS: super::super::memo::ScanMemo<Scan> = super::super::memo::ScanMemo::new();

/// The scan inputs a `match` snapshot does not digest; a stored scan is
/// served only to a page with the same ones. The row shape is one of them:
/// a priced scan's prices are the sizes of rows of that shape.
pub(super) fn scan_key(q: MatchQuery<'_>, paths: &PathPolicy) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        paths.identity(),
        q.default_excludes(),
        octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        q.capture_text().unwrap_or(false),
        q.provenance(),
        q.test_code()
    ]))
}

/// Rough heap bytes of a stored scan, its memo weight.
pub(super) fn scan_bytes(scan: &Scan) -> usize {
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
            .chain(&scan.earlier_files)
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
    if memo_source_unchanged(scope, &scan) {
        Some(scan)
    } else {
        SCANS.evict(snapshot);
        None
    }
}

/// A fresh scan of the row's scope (its files in walk order, unsorted: see
/// [`super::scan::sort_files`]) and when it started. Every request owns its
/// scan: identical concurrent requests do not share one.
pub(super) fn fresh_scan(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(Scan, std::time::SystemTime), ToolError> {
    let started = octocode_engine::stamp::settle_clock();
    let scan = if scope.is_file {
        scan_file(q, scope, paths, cancel)?
    } else {
        scan_directory(q, scope, paths, cancel, None)?
    };
    cancel.check().map_err(ToolError::cancelled)?;
    Ok((scan, started))
}

/// Keep a fresh scan for the continuation pages of its result, watched for
/// changes to every source it read.
///
/// Only a scan with later pages asks what its directories' ignore files are:
/// one written since the walk started leaves the proof untaken, so asking after
/// the walk proves as much. An ignore file no stamp can watch (a link, an
/// unreadable entry) leaves the scan unkept.
pub(super) fn remember_scan(
    (snapshot, key): (String, String),
    scope: &Scope,
    scan: &std::sync::Arc<Scan>,
    paths: &PathPolicy,
    started: std::time::SystemTime,
) {
    let mut watched = scan
        .sources
        .iter()
        .chain(&scan.discovery_sources)
        .chain(&scan.earlier_files)
        .cloned()
        .collect::<Vec<_>>();
    let mut rules = scan.ignore_rules.clone();
    if scan.watches_ignore_files {
        let discovery = paths.discovery_walk();
        for dir in &scan.discovery_sources {
            let inputs = octocode_tools_shared::page_memo::directory_ignore_inputs_with(
                dir,
                &scope.canonical,
                paths.global_ignore(),
                |ignore| discovery.permits_entry(ignore, None),
            );
            if inputs.untracked {
                return;
            }
            watched.extend(inputs.files);
            rules.extend(inputs.external);
        }
    }
    SCANS.put(
        (snapshot, key),
        octocode_tools_shared::page_memo::Watch {
            root: &scope.canonical,
            sources: &watched,
            rules: &rules,
            markers: &scan.repository_markers,
        },
        started,
        std::sync::Arc::clone(scan),
        scan_bytes(scan),
    );
}
