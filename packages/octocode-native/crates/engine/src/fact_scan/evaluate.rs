//! Reading, parsing and mapping each walked file of a graph-facts scan.

use std::{path::Path, sync::Arc};

use rayon::prelude::*;

use super::memo::{FACTS_MEMO, ParsedFacts, facts_heap_bytes};
use super::{
    FactsSource, GraphFactsMappedScan, GraphFactsView, GraphScanError, MAX_GRAPH_SOURCE_BYTES,
    ParserOverride, parser_overrides,
};
use crate::search::walk::Stop;
use crate::stamp::SourceStamp;
use crate::types::{GraphFactsScanDiagnostic, GraphFactsScanOptions};

enum GraphFactsScanOutcome<T> {
    /// A parsed file and what the caller's map kept of it (`None`: nothing).
    Entry(String, Option<T>),
    Skipped(GraphFactsScanDiagnostic),
}

fn skipped<T>(relative_path: String, code: &str, message: &str) -> GraphFactsScanOutcome<T> {
    GraphFactsScanOutcome::Skipped(GraphFactsScanDiagnostic {
        relative_path,
        code: code.to_owned(),
        message: message.to_owned(),
    })
}

/// Read, parse and map the walked candidates of a scan, within its time
/// budget when `max_files` is unset.
pub(super) fn evaluate_walk<T: Send>(
    options: GraphFactsScanOptions,
    query: crate::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    budget: Option<std::time::Duration>,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    let root = std::path::PathBuf::from(&options.path);
    let overrides = parser_overrides(&root, options.language_globs.as_deref())?;
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
        overrides: &overrides,
        max_file_bytes,
        source,
        keep,
        map,
        stop,
    };
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
        for outcome in chunk
            .par_iter()
            .map(|entry| evaluator.evaluate(entry))
            .collect::<Result<Vec<_>, GraphScanError>>()?
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
    // The walk cap of an unset `max_files` resumes where the walk stopped.
    if timed && scan_resume.is_none() && truncated {
        scan_resume = Some(u32::try_from(skip + evaluated).unwrap_or(u32::MAX));
    }
    let truncated = truncated || scan_resume.is_some();
    kept.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    skipped.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let files_skipped = skipped.len() as u32;

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
    settle_now: std::time::SystemTime,
    overrides: &'s [ParserOverride],
    max_file_bytes: u32,
    source: FactsSource<'s>,
    keep: &'s (dyn Fn(&str) -> bool + Sync),
    map: &'s (dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &'s Stop<'s>,
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
        let Ok(admitted) = source.admit(crate::text::Decoding::Unicode) else {
            return outcome(
                "graph.scan.readFailed",
                "file could not be admitted as text (binary, or not UTF-8 or BOM-marked UTF-16)",
            );
        };
        let content = admitted.text;
        let metadata = &source.metadata;
        if !(self.keep)(&content) {
            return Ok(None);
        }
        let selected = self
            .overrides
            .iter()
            .filter(|(_, matcher)| crate::search::glob::matches(matcher, path, false))
            .map(|(extension, _)| extension.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if selected.len() > 1 {
            return outcome(
                "graph.scan.ambiguousParser",
                "languageGlobs matched this path with more than one parser",
            );
        }
        // The digest and the settled stamp identify the file's bytes; the
        // decoded text only fixes the coordinates of its facts.
        let content_digest = crate::digest::sha256(&source.bytes);
        // A file written while it was read is not settled.
        let source_stamp = Some(metadata)
            .filter(|meta| meta.len() == source.bytes.len() as u64)
            .and_then(|meta| SourceStamp::settled(meta, self.settle_now));
        let parser = selected.iter().next().copied();
        if let FactsSource::Declarations(extract) = self.source {
            let Some(facts) = extract(&content, path, parser) else {
                return outcome(
                    "graph.scan.extractFailed",
                    "native graph-fact extraction returned no result",
                );
            };
            let kept = (self.map)(&GraphFactsView {
                relative_path: &relative_path,
                content: &content,
                content_digest: &content_digest,
                source_stamp,
                facts: &facts,
                reference_counts: &[],
            });
            return Ok(Some(GraphFactsScanOutcome::Entry(relative_path, kept)));
        }
        let memo_key = format!(
            "{content_digest}\u{0}{relative_path}\u{0}{}",
            parser.unwrap_or("")
        );
        let parsed = if let Some(hit) = FACTS_MEMO.get(&memo_key) {
            hit
        } else {
            let extraction = if let Some(extension) = parser {
                crate::signatures::graph_facts::extract_graph_facts_with_metadata_with_extension(
                    &content,
                    &relative_path,
                    extension,
                )
            } else {
                crate::signatures::extract_graph_facts_with_metadata_inner(&content, &relative_path)
            };
            let Some(extraction) = extraction else {
                return outcome(
                    "graph.scan.extractFailed",
                    "native graph-fact extraction returned no result",
                );
            };
            let weight = facts_heap_bytes(&extraction.facts, &extraction.reference_counts);
            let parsed = Arc::new(ParsedFacts {
                facts: extraction.facts,
                reference_counts: extraction.reference_counts,
                weight: u32::try_from(weight).unwrap_or(u32::MAX),
            });
            FACTS_MEMO.insert(memo_key, Arc::clone(&parsed));
            parsed
        };
        let kept = (self.map)(&GraphFactsView {
            relative_path: &relative_path,
            content: &content,
            content_digest: &content_digest,
            source_stamp,
            facts: &parsed.facts,
            reference_counts: &parsed.reference_counts,
        });
        Ok(Some(GraphFactsScanOutcome::Entry(relative_path, kept)))
    }
}
