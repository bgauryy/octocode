//! Reading, parsing and mapping each walked file of a graph-facts scan.

use std::{path::Path, sync::Arc};

use rayon::prelude::*;

mod cached;
mod outcome;
use super::{
    FactsSource, GraphFactsMappedScan, GraphFactsView, GraphScanError, MAX_GRAPH_SOURCE_BYTES,
    ParserOverride, parser_overrides_with_registry,
};
use crate::search::walk::Stop;
use crate::stamp::SourceStamp;
use crate::types::GraphFactsScanOptions;
pub use outcome::SOURCE_BUDGET_EXCEEDED;
use outcome::{GraphFactsScanOutcome, Mapped, mapped_outcome, skipped};

use crate::signatures::languages::{RegistryEntry, RegistryView};
use crate::text::analysis::{AnalysisOperation, MapLimits};

/// Read, parse and map the walked candidates of a scan, within its time
/// budget when `max_files` is unset.
#[cfg(test)]
pub(super) fn evaluate_walk<T: Send>(
    options: GraphFactsScanOptions,
    query: crate::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    budget: Option<std::time::Duration>,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    evaluate_walk_with_registry(
        options,
        query,
        source,
        keep,
        map,
        stop,
        budget,
        RegistryView::installed(),
        MapLimits::default(),
    )
}

pub(super) fn evaluate_walk_with_registry<T: Send>(
    options: GraphFactsScanOptions,
    query: crate::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    budget: Option<std::time::Duration>,
    registry: RegistryView<'_>,
    map_limits: MapLimits,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    let root = std::path::PathBuf::from(&options.path);
    let overrides =
        parser_overrides_with_registry(&root, options.language_globs.as_deref(), registry)?;
    let timed = options.max_files.is_none();
    let max_file_bytes = options.max_file_bytes.unwrap_or(MAX_GRAPH_SOURCE_BYTES);
    let truncated = query.was_capped;
    let mut entries = query.entries;
    // Walk order is stable, so the leading files an earlier window read are
    // exactly these.
    let skip = options
        .skip_files
        .map_or(0, |n| n as usize)
        .min(entries.len());
    entries.drain(..skip);
    let evaluator = EntryEvaluator {
        // Read once, on the calling thread: a file written after the scan
        // began is not settled, and parallel reads share one clock.
        settle_now: crate::stamp::settle_clock(),
        registry,
        map_limits,
        overrides: &overrides,
        max_file_bytes,
        source,
        keep,
        map,
        stop,
    };
    // Raised once `stop` says the scan is cancelled: a file's extraction runs to
    // completion unless then (no clock cuts a file; the pacer pages between files).
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut pacer = crate::scan_budget::ScanPacer::new(budget.filter(|_| timed));
    let mut kept: Vec<(String, T)> = Vec::new();
    let mut skipped = Vec::new();
    let (mut evaluated, mut parsed) = (0, 0);
    let mut scan_resume = None;
    let mut rest = entries.as_slice();
    while !rest.is_empty() {
        if pacer.spent() {
            scan_resume = Some(u32::try_from(skip + evaluated).unwrap_or(u32::MAX));
            break;
        }
        let (chunk, tail) = rest.split_at(pacer.next_chunk().min(rest.len()));
        rest = tail;
        evaluated += chunk.len();
        pacer.record(chunk.len());
        for outcome in watch_stop(stop, &stopped, &|| {
            chunk
                .par_iter()
                .map(|entry| {
                    crate::deep_stack::while_stoppable(&stopped, || evaluator.evaluate(entry))
                })
                .collect::<Result<Vec<_>, GraphScanError>>()
        })?
        .into_iter()
        .flatten()
        {
            match outcome {
                GraphFactsScanOutcome::Entry(path, value) => {
                    parsed += 1;
                    kept.extend(value.map(|value| (path, value)));
                }
                GraphFactsScanOutcome::Skipped(diagnostic) => skipped.push(diagnostic),
            }
        }
    }
    // Every cut resumes where it stopped: the walk cap of an unset
    // `max_files`, and a caller's file window.
    if scan_resume.is_none() && truncated {
        scan_resume = Some(u32::try_from(skip + evaluated).unwrap_or(u32::MAX));
    }
    let truncated = truncated || scan_resume.is_some();
    kept.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    skipped.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let files_skipped = crate::num::saturating_u32(skipped.len());

    Ok(GraphFactsMappedScan {
        kept,
        parsed,
        skipped,
        files_skipped,
        truncated,
        scan_resume,
        scan_frontier: query.first_unlisted,
        pruned_dirs: query.pruned_dirs,
    })
}

/// What evaluating one walked file needs: the settle time, the byte limit,
/// the parser overrides, and the caller's prefilter, map and stop.
struct EntryEvaluator<'s, T> {
    registry: RegistryView<'s>,
    map_limits: MapLimits,
    settle_now: std::time::SystemTime,
    overrides: &'s [ParserOverride],
    max_file_bytes: u32,
    source: FactsSource<'s>,
    keep: &'s (dyn Fn(&str) -> bool + Sync),
    map: &'s (dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &'s Stop<'s>,
}

struct MappedFacts<'a> {
    facts: &'a crate::graph::GraphFactsDocument,
    reference_counts: &'a [crate::graph::GraphReferenceCount],
    reused: bool,
}

impl<T> EntryEvaluator<'_, T> {
    /// Reads one walked file and maps its facts, parsed or reused from the
    /// memo: `None` when `keep` rejects its source, a skip diagnostic when it
    /// cannot be read, matches several parsers, or yields no facts.
    fn evaluate(
        &self,
        entry: &crate::types::FileSystemEntry,
    ) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
        if (self.stop)() {
            return Err(GraphScanError::Failed("cancelled".to_owned()));
        }
        let path = Path::new(&entry.path);
        let relative_path = entry.relative_path.replace('\\', "/");
        let outcome =
            |code: &str, message: &str| Ok(Some(skipped(relative_path.clone(), code, message)));
        let too_large = || {
            outcome(
                "graph.scan.fileTooLarge",
                "file exceeds the graph scan byte limit",
            )
        };
        // The walk's size is advisory: the read is bounded on its own.
        if entry.size.unwrap_or_default() > i64::from(self.max_file_bytes) {
            return too_large();
        }
        let source = match crate::text::read_source(path, u64::from(self.max_file_bytes)) {
            Ok(source) => source,
            Err(crate::text::ReadError::TooLarge(_)) => return too_large(),
            Err(_) => return outcome("graph.scan.readFailed", "file could not be opened"),
        };
        let cancelled = || (self.stop)();
        // Admission is bounded by the map limits and the scan's cancellation, not a clock.
        let mut map_budget = crate::text::analysis::MapBudget::new(
            self.map_limits,
            crate::signatures::extractor::whole_file_deadline(),
            &cancelled,
        );
        let admitted = match crate::text::analysis::AdmittedSource::from_bytes(
            &source.bytes,
            &path
                .canonicalize()
                .map_err(|error| GraphScanError::Failed(format!("source identity: {error}")))?
                .to_string_lossy(),
            &mut map_budget,
        ) {
            Ok(admitted) => admitted,
            Err(crate::text::analysis::SourceError::Refused(_)) => {
                return outcome(
                    "graph.scan.readFailed",
                    "file could not be admitted as text (binary, or not UTF-8 or BOM-marked UTF-16)",
                );
            }
            Err(error) => return Err(GraphScanError::Source(error)),
        };
        let content = admitted.text();
        if !(self.keep)(content) {
            return Ok(None);
        }
        let Ok(parser) = self.selected_parser(path) else {
            return outcome(
                "graph.scan.ambiguousParser",
                "languageGlobs matched this path with more than one parser",
            );
        };
        // The digest and the settled stamp identify the file's bytes; the
        // decoded text only fixes the coordinates of its facts.
        let content_digest = admitted
            .source_digest()
            .map_or_else(|| crate::digest::sha256(&source.bytes), str::to_owned);
        let source_stamp = self.source_stamp(&source.metadata, source.bytes.len());
        let extension = crate::text::extension_of(&relative_path, true, "");
        let grammar_extension = crate::text::file_extension::grammar_extension(content, &extension);
        let selected_entry = self.registry.find(parser.unwrap_or(grammar_extension));
        let operation = if matches!(self.source, FactsSource::Declarations(_)) {
            AnalysisOperation::Declarations
        } else {
            AnalysisOperation::GraphFacts
        };
        let Some(selected_entry) = selected_entry.filter(|entry| entry.admits(operation)) else {
            return outcome(
                "graph.scan.operationUnsupported",
                "selected provider does not admit this analysis operation",
            );
        };
        if let RegistryEntry::Facts(provider) = selected_entry {
            let Some((entry, reused)) = self.provider_facts(
                provider,
                &admitted,
                &relative_path,
                parser,
                &content_digest,
                operation,
            )?
            else {
                return outcome(
                    "graph.scan.extractFailed",
                    "facts provider returned no result",
                );
            };
            let kept = self.map_facts(
                MappedFacts {
                    facts: &entry.facts,
                    reference_counts: &entry.reference_counts,
                    reused,
                },
                &admitted,
                &relative_path,
                &content_digest,
                source_stamp,
                &mut map_budget,
            )?;
            return mapped_outcome(relative_path, kept);
        }
        if let FactsSource::Declarations(extract) = self.source {
            let Some(facts) = extract(content, path, parser) else {
                return outcome(
                    "graph.scan.extractFailed",
                    "native graph-fact extraction returned no result",
                );
            };
            let kept = self.map_facts(
                MappedFacts {
                    facts: &facts,
                    reference_counts: &[],
                    reused: false,
                },
                &admitted,
                &relative_path,
                &content_digest,
                source_stamp,
                &mut map_budget,
            )?;
            return mapped_outcome(relative_path, kept);
        }
        let Some((parsed, reused)) = self.cached_facts(
            &admitted,
            &relative_path,
            parser,
            selected_entry,
            &content_digest,
        ) else {
            return outcome(
                "graph.scan.extractFailed",
                "native graph-fact extraction returned no result",
            );
        };
        let kept = self.map_facts(
            MappedFacts {
                facts: &parsed.facts,
                reference_counts: &parsed.reference_counts,
                reused,
            },
            &admitted,
            &relative_path,
            &content_digest,
            source_stamp,
            &mut map_budget,
        )?;
        mapped_outcome(relative_path, kept)
    }

    fn selected_parser(&self, path: &Path) -> Result<Option<&str>, ()> {
        let selected = self
            .overrides
            .iter()
            .filter(|(_, matcher)| crate::search::glob::matches(matcher, path, false))
            .map(|(extension, _)| extension.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if selected.len() > 1 {
            Err(())
        } else {
            Ok(selected.first().copied())
        }
    }

    fn source_stamp(&self, metadata: &std::fs::Metadata, bytes: usize) -> Option<SourceStamp> {
        // A file written while it was read is not settled.
        (metadata.len() == bytes as u64)
            .then(|| SourceStamp::settled(metadata, self.settle_now))
            .flatten()
    }

    fn map_facts(
        &self,
        mapped: MappedFacts<'_>,
        admitted: &crate::text::analysis::AdmittedSource<'_>,
        relative_path: &str,
        content_digest: &str,
        source_stamp: Option<SourceStamp>,
        map_budget: &mut crate::text::analysis::MapBudget<'_>,
    ) -> Result<Mapped<T>, GraphScanError> {
        let source = match mapped.facts.source_templates.bind(admitted, map_budget) {
            Ok(source) => source,
            Err(crate::text::analysis::SourceError::LimitExceeded(_)) => {
                return Ok(Mapped::OverBudget);
            }
            Err(error) => return Err(GraphScanError::Source(error)),
        };
        Ok(Mapped::Kept((self.map)(&GraphFactsView {
            admitted,
            source: &source,
            relative_path,
            content: admitted.text(),
            content_digest,
            source_stamp,
            facts: mapped.facts,
            reference_counts: mapped.reference_counts,
            reused: mapped.reused,
        })))
    }
}

/// Run `work` while a scoped watcher raises `stopped` once `stop` reports the
/// scan cancelled, so deep-stack jobs of the work stop with the scan.
fn watch_stop<R>(
    stop: &Stop<'_>,
    stopped: &Arc<std::sync::atomic::AtomicBool>,
    work: &dyn Fn() -> R,
) -> R {
    use std::sync::atomic::{AtomicBool, Ordering};
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if stop() {
                    stopped.store(true, Ordering::Relaxed);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        let result = work();
        done.store(true, Ordering::Relaxed);
        result
    })
}
