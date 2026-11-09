//! Reading, parsing and mapping each walked file of a graph-facts scan.

use std::{path::Path, sync::Arc};

mod facts;
mod ordered;
mod outcome;
use super::memory::{FileHold, ScanMemory};
use super::{
    FactsSource, GraphFactsMappedScan, GraphFactsView, GraphScanError, ParserOverride, ScanHold,
    parser_overrides_with_registry,
};
use crate::search::walk::Stop;
use crate::stamp::SourceStamp;
use crate::types::GraphFactsScanOptions;
use ordered::watch_stop;
use outcome::{GraphFactsScanOutcome, Mapped, contained, failed, mapped_outcome, skipped};
pub use outcome::{PANICKED, SOURCE_BUDGET_EXCEEDED, over_budget_message, panicked};

use crate::contain::{Phase, contain, observe, within};

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
        (budget, None),
        RegistryView::installed(),
        (MapLimits::default(), None),
    )
}

pub(super) fn evaluate_walk_with_registry<T: Send>(
    options: GraphFactsScanOptions,
    query: crate::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    (budget, hold): (Option<std::time::Duration>, Option<&ScanHold<'_, T>>),
    registry: RegistryView<'_>,
    (map_limits, memory): (MapLimits, Option<&ScanMemory<'_>>),
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    let root = std::path::PathBuf::from(&options.path);
    let overrides =
        parser_overrides_with_registry(&root, options.language_globs.as_deref(), registry)?;
    let timed = options.max_files.is_none();
    let max_file_bytes = options.max_file_bytes.unwrap_or(u64::MAX);
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
        root: Root {
            canonical: root.canonicalize().ok(),
            given: &root,
        },
        // Read once, on the calling thread: a file written after the scan
        // began is not settled, and parallel reads share one clock.
        settle_now: crate::stamp::settle_clock(),
        registry,
        map_limits,
        memory,
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
    let (mut evaluated, mut parsed, mut passed_over) = (0, 0, 0);
    let limit = hold.map_or(usize::MAX, |hold| hold.limit);
    let mut held = 0_usize;
    let mut scan_resume = None;
    let mut rest = entries.as_slice();
    while !rest.is_empty() {
        if pacer.spent() || held > limit {
            scan_resume = Some(u32::try_from(skip + evaluated).unwrap_or(u32::MAX));
            break;
        }
        let chunk = &rest[..pacer.next_chunk().min(rest.len())];
        let outcomes = watch_stop(stop, &stopped, &|| {
            ordered::weighed_chunk(
                chunk,
                (held, limit),
                hold,
                |entry| crate::deep_stack::while_stoppable(&stopped, || evaluator.evaluate(entry)),
                |entry, panic| {
                    GraphFactsScanOutcome::Skipped(outcome::panicked(
                        crate::text::slash_path(Path::new(&entry.relative_path)),
                        panic,
                    ))
                },
            )
        })?;
        // Where the running total in walk order passes the limit is a fact of the
        // files, so a page that scans again stops at the same one; what the workers
        // finished past it is the next window's.
        let mut taken = 0;
        for (weight, outcome) in outcomes {
            taken += 1;
            held = held.saturating_add(weight);
            match outcome {
                None => passed_over += 1,
                Some(GraphFactsScanOutcome::Entry(path, value)) => {
                    parsed += 1;
                    kept.extend(value.map(|value| (path, value)));
                }
                Some(GraphFactsScanOutcome::Skipped(diagnostic)) => skipped.push(diagnostic),
            }
            if held > limit {
                break;
            }
        }
        rest = &rest[taken..];
        evaluated += taken;
        pacer.record(taken);
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
        passed_over,
        skipped,
        files_skipped,
        truncated,
        scan_resume,
        scan_frontier: query.first_unlisted,
        pruned_dirs: query.pruned_dirs,
    })
}

/// Test-only: a file whose relative path holds one of these markers panics
/// in its decode step.
#[cfg(test)]
pub(super) static DECODE_FAULTS: std::sync::Mutex<Vec<&'static str>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn decode_fault(relative_path: &str) {
    let faulted = DECODE_FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|marker| relative_path.contains(marker));
    assert!(!faulted, "injected decode fault");
}

/// The walk's root as the caller spelled it, and as the file system names it.
struct Root<'s> {
    given: &'s Path,
    canonical: Option<std::path::PathBuf>,
}

impl Root<'_> {
    /// The canonical path of the walked file at `path`. The walk lists a
    /// file under the root without following a link (and a listed link is
    /// refused when read), so the canonical root plus the file's path under
    /// the root names it: one `realpath` for the walk, not one per file. A
    /// path not under the root is resolved itself.
    fn identity(&self, path: &Path) -> std::io::Result<std::path::PathBuf> {
        match (&self.canonical, path.strip_prefix(self.given)) {
            (Some(canonical), Ok(under)) if under.as_os_str().is_empty() => Ok(canonical.clone()),
            (Some(canonical), Ok(under)) => Ok(canonical.join(under)),
            _ => path.canonicalize(),
        }
    }
}

/// What evaluating one walked file needs: the settle time, the caller's read ceiling,
/// the parser overrides, and the caller's prefilter, map and stop.
struct EntryEvaluator<'s, T> {
    root: Root<'s>,
    registry: RegistryView<'s>,
    map_limits: MapLimits,
    memory: Option<&'s ScanMemory<'s>>,
    settle_now: std::time::SystemTime,
    overrides: &'s [ParserOverride],
    max_file_bytes: u64,
    source: FactsSource<'s>,
    keep: &'s (dyn Fn(&str) -> bool + Sync),
    map: &'s (dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &'s Stop<'s>,
}

/// One file's mapping, its panic or failure on the file itself returned
/// as the file's named skip: the scan goes on.
macro_rules! guarded {
    ($path:ident, $work:expr) => {
        match contain(Phase::Map, || $work) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => return failed($path, Phase::Map, error),
            Err(panic) => return contained($path, &panic),
        }
    };
}

/// [`guarded`] for extraction, which has designed fallbacks (oxc to
/// tree-sitter): a panic it recovered from voids the file only when no
/// facts came of it.
macro_rules! extracted {
    ($path:ident, $work:expr) => {
        match observe(Phase::Extract, || $work) {
            Ok((Ok(None), Some(panic))) | Err(panic) => return contained($path, &panic),
            Ok((Ok(value), _)) => value,
            Ok((Err(error), _)) => return failed($path, Phase::Extract, error),
        }
    };
}

#[derive(Clone, Copy)]
struct MappedFacts<'a> {
    facts: &'a crate::graph::GraphFactsDocument,
    reference_counts: &'a [crate::graph::GraphReferenceCount],
    reused: bool,
}

impl<T> EntryEvaluator<'_, T> {
    /// Reads one walked file and maps its facts, parsed or reused from the
    /// caller's checkpoint: `None` when `keep` rejects its source, a skip diagnostic when it
    /// cannot be read, matches several parsers, yields no facts, or its
    /// extraction or mapping panics or fails on the file itself.
    fn evaluate(
        &self,
        entry: &crate::types::FileSystemEntry,
    ) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
        if (self.stop)() {
            return Err(GraphScanError::Failed("cancelled".to_owned()));
        }
        let path = Path::new(&entry.path);
        let relative_path = crate::text::slash_path(Path::new(&entry.relative_path));
        let outcome =
            |code: &str, message: &str| Ok(Some(skipped(relative_path.clone(), code, message)));
        let too_large = |size: u64| {
            outcome(
                "graph.scan.fileTooLarge",
                &format!(
                    "file of {size} bytes is larger than the {} bytes this scan may hold whole",
                    self.max_file_bytes
                ),
            )
        };
        // The walk's size is advisory: the read is bounded on its own.
        if let Some(size) = entry
            .size
            .and_then(|size| u64::try_from(size).ok())
            .filter(|size| *size > self.max_file_bytes)
        {
            return too_large(size);
        }
        let source = match within(Phase::Read, || {
            crate::text::read_regular(path, self.max_file_bytes)
        }) {
            Ok(source) => source,
            Err(crate::text::ReadError::TooLarge(size)) => return too_large(size),
            Err(_) => return outcome("graph.scan.readFailed", "file could not be opened"),
        };
        let cancelled = || (self.stop)();
        // Admission is bounded by the map limits and the scan's cancellation, not a clock.
        let mut map_budget = crate::text::analysis::MapBudget::new(
            self.map_limits,
            crate::signatures::extractor::whole_file_deadline(),
            &cancelled,
        );
        let identity = self
            .root
            .identity(path)
            .map_err(|error| GraphScanError::Failed(format!("source identity: {error}")))?;
        let admitted = match within(Phase::Decode, || {
            #[cfg(test)]
            decode_fault(&relative_path);
            crate::text::analysis::AdmittedSource::from_bytes(
                &source.bytes,
                &identity.to_string_lossy(),
                &mut map_budget,
            )
        }) {
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
        if !within(Phase::Keep, || (self.keep)(content)) {
            return Ok(None);
        }
        self.evaluate_admitted(
            path,
            relative_path,
            (&source.bytes, &source.metadata),
            &admitted,
            &mut map_budget,
        )
    }

    /// [`Self::evaluate`] of a file admitted as text that `keep` kept.
    fn evaluate_admitted(
        &self,
        path: &Path,
        relative_path: String,
        (bytes, metadata): (&[u8], &std::fs::Metadata),
        admitted: &crate::text::analysis::AdmittedSource<'_>,
        map_budget: &mut crate::text::analysis::MapBudget<'_>,
    ) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
        let content = admitted.text();
        let outcome =
            |code: &str, message: &str| Ok(Some(skipped(relative_path.clone(), code, message)));
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
            .map_or_else(|| crate::digest::sha256(bytes), str::to_owned);
        let source_stamp = self.source_stamp(metadata, bytes.len());
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
            let Some((entry, reused)) = extracted!(
                relative_path,
                self.provider_facts(
                    provider,
                    admitted,
                    &relative_path,
                    parser,
                    &content_digest,
                    operation,
                )
            ) else {
                return outcome(
                    "graph.scan.extractFailed",
                    "facts provider returned no result",
                );
            };
            let kept = guarded!(
                relative_path,
                self.map_facts(
                    MappedFacts {
                        facts: &entry.facts,
                        reference_counts: &entry.reference_counts,
                        reused,
                    },
                    admitted,
                    &relative_path,
                    &content_digest,
                    source_stamp,
                    &mut *map_budget,
                )
            );
            return mapped_outcome(relative_path, kept);
        }
        if let FactsSource::Declarations(extract) = self.source {
            let Some(facts) = extracted!(
                relative_path,
                Ok::<_, GraphScanError>(extract(content, path, parser))
            ) else {
                return outcome(
                    "graph.scan.extractFailed",
                    "native graph-fact extraction returned no result",
                );
            };
            let kept = guarded!(
                relative_path,
                self.map_facts(
                    MappedFacts {
                        facts: &facts,
                        reference_counts: &[],
                        reused: false,
                    },
                    admitted,
                    &relative_path,
                    &content_digest,
                    source_stamp,
                    &mut *map_budget,
                )
            );
            return mapped_outcome(relative_path, kept);
        }
        let Some((parsed, reused)) = extracted!(
            relative_path,
            Ok::<_, GraphScanError>(self.extract_facts(
                admitted,
                &relative_path,
                parser,
                &content_digest
            ))
        ) else {
            return outcome(
                "graph.scan.extractFailed",
                "native graph-fact extraction returned no result",
            );
        };
        let kept = guarded!(
            relative_path,
            self.map_facts(
                MappedFacts {
                    facts: &parsed.facts,
                    reference_counts: &parsed.reference_counts,
                    reused,
                },
                admitted,
                &relative_path,
                &content_digest,
                source_stamp,
                &mut *map_budget,
            )
        );
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

    /// Binds the file's source and runs the caller's map. Under a shared
    /// [`ScanMemory`] the file first fits its facts beside what the scan
    /// kept (else the scan is refused, as its caller's admission would
    /// refuse them), then draws every byte of its correspondence; a draw
    /// refused only because other files held the memory runs the file again
    /// alone. A file whose correspondence does not fit even so is the scan's
    /// refusal when the scan kept files (the scope is too broad for the
    /// build), else its named skip (no build could hold it).
    fn map_facts(
        &self,
        mapped: MappedFacts<'_>,
        admitted: &crate::text::analysis::AdmittedSource<'_>,
        relative_path: &str,
        content_digest: &str,
        source_stamp: Option<SourceStamp>,
        map_budget: &mut crate::text::analysis::MapBudget<'_>,
    ) -> Result<Mapped<T>, GraphScanError> {
        let file = (admitted, relative_path, content_digest, source_stamp);
        let Some(memory) = self.memory else {
            return self.bind_and_map(mapped, file, map_budget, None);
        };
        let base = memory.base(mapped.facts);
        let kept = memory.committed();
        if kept.saturating_add(base) > memory.budget() {
            memory.refuse(relative_path, kept.saturating_add(base));
            return Ok(Mapped::Refused);
        }
        let hold = FileHold::priced(memory, base);
        let attempt = || {
            use crate::text::analysis::Draw;
            if hold.draw(base).is_err() {
                return Ok(Mapped::OverBudget(
                    "bytes",
                    format!(
                        "needs more than the {} bytes the scan may hold ({} for its facts)",
                        memory.budget(),
                        base
                    ),
                ));
            }
            let mut budget = map_budget.fresh().drawing(Some(&hold));
            self.bind_and_map(mapped, file, &mut budget, Some(&hold))
        };
        let mut outcome = attempt()?;
        if hold.take_contended() {
            outcome = memory.alone(&hold, attempt)?;
        }
        // Only the shared bytes depend on what else the scan kept; a file
        // past a limit of its own shape is its skip wherever it is built.
        if matches!(outcome, Mapped::OverBudget("bytes", _)) && memory.committed() > 0 {
            memory.refuse(relative_path, hold.needed());
            return Ok(Mapped::Refused);
        }
        Ok(outcome)
    }

    fn bind_and_map(
        &self,
        mapped: MappedFacts<'_>,
        (admitted, relative_path, content_digest, source_stamp): (
            &crate::text::analysis::AdmittedSource<'_>,
            &str,
            &str,
            Option<SourceStamp>,
        ),
        map_budget: &mut crate::text::analysis::MapBudget<'_>,
        hold: Option<&FileHold<'_>>,
    ) -> Result<Mapped<T>, GraphScanError> {
        let source = match mapped.facts.source_templates.bind(admitted, map_budget) {
            Ok(source) => source,
            Err(crate::text::analysis::SourceError::LimitExceeded(name)) => {
                return Ok(Mapped::OverBudget(name, map_budget.exceeded(name)));
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
            memory: hold,
        })))
    }
}
