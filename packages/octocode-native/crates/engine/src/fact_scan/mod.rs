//! The graph-facts scan: walks a tree for files of an outlined grammar,
//! reads and parses each one (reusing unchanged files' facts), and hands the
//! facts to the caller's map. The graph types it fills live in
//! [`crate::graph`]; the extraction lives in [`crate::signatures`].
use std::{fs, path::Path, sync::Arc};

use ignore::gitignore::Gitignore;
use rayon::prelude::*;

use crate::error::ErrorKind;
use crate::graph::{
    GraphFactCall, GraphFactCommonJs, GraphFactDeclaration, GraphFactEdge, GraphFactExport,
    GraphFactImport, GraphFactPath, GraphFactRustModule, GraphFactsDocument, GraphFactsTypedEntry,
};
use crate::search::walk::Stop;
use crate::stamp::SourceStamp;
use crate::types::{FileSystemQueryOptions, GraphFactsScanDiagnostic, GraphFactsScanOptions};

/// Largest source the graph-facts scan parses; a larger file stays an
/// unparsed (skipped) node. One bound for graph ingest and its reuse check.
pub const MAX_GRAPH_SOURCE_BYTES: u32 = 1_000_000;

enum GraphFactsScanOutcome<T> {
    /// A parsed file and what the caller's map kept of it (`None`: nothing).
    Entry(String, Option<T>),
    Skipped(GraphFactsScanDiagnostic),
}

/// One parsed file, borrowed from the scan's cache: a caller reduces it to
/// what it keeps before the next file is read.
pub struct GraphFactsView<'a> {
    pub relative_path: &'a str,
    /// The source the facts were parsed from (the one read of the file).
    pub content: &'a str,
    pub content_digest: &'a str,
    pub source_stamp: Option<SourceStamp>,
    pub facts: &'a GraphFactsDocument,
    pub reference_counts: &'a [crate::types::GraphReferenceCount],
}

impl GraphFactsView<'_> {
    /// The whole parsed file, owned.
    pub fn to_entry(&self) -> GraphFactsTypedEntry {
        GraphFactsTypedEntry {
            relative_path: self.relative_path.to_owned(),
            content_digest: self.content_digest.to_owned(),
            source_stamp: self.source_stamp,
            facts: self.facts.clone(),
            reference_counts: self.reference_counts.to_vec(),
        }
    }
}

/// The result of [`scan_walked_graph_facts`]: only what the map kept.
#[derive(Debug)]
pub struct GraphFactsMappedScan<T> {
    /// `(relative path, kept value)` per file the map kept, sorted by path.
    pub kept: Vec<(String, T)>,
    /// Files read and parsed (kept or not); skipped files are separate.
    pub parsed: usize,
    pub skipped: Vec<GraphFactsScanDiagnostic>,
    pub files_skipped: u32,
    pub truncated: bool,
    pub scan_resume: Option<u32>,
    /// The first candidate (absolute path, walk order) the walk cap left
    /// out: it and every later candidate are unread.
    pub scan_frontier: Option<String>,
    pub pruned_dirs: Vec<String>,
}

/// Why a graph scan failed. The two `languageGlobs` refusals are typed so the
/// runtime maps them to their `errorCode` without reading message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphScanError {
    /// A `languageGlobs` pattern is malformed or over its limits.
    InvalidGlob(String),
    /// A `languageGlobs` key selects no single registered grammar.
    UnsupportedLanguage(String),
    /// Walking or reading the tree failed, including a caller's path check.
    Failed(String),
}

impl std::fmt::Display for GraphScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGlob(message) => write!(f, "[ast.language.invalidGlob] {message}"),
            Self::UnsupportedLanguage(message) => write!(f, "[ast.language.unsupported] {message}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for GraphScanError {}

impl From<String> for GraphScanError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

/// One `languageGlobs` rule: the parser extension it selects, and its glob
/// (see [`crate::search::glob`]).
pub type ParserOverride = (String, Gitignore);

/// Compiles `languageGlobs` rules against `root`; astSearch `match` selects
/// its parsers with the same rules.
pub fn parser_overrides(
    root: &Path,
    rules: Option<&[crate::types::GraphLanguageGlob]>,
) -> Result<Vec<ParserOverride>, GraphScanError> {
    if rules.unwrap_or_default().len() > 400 {
        return Err(GraphScanError::InvalidGlob(
            "languageGlobs exceeds 400 patterns".into(),
        ));
    }
    rules
        .unwrap_or_default()
        .iter()
        .map(|rule| {
            if rule.glob.len() > 256 || rule.glob.is_empty() {
                return Err(GraphScanError::InvalidGlob(
                    "glob must contain 1-256 characters".into(),
                ));
            }
            let extension = crate::signatures::languages::select_one(&rule.language)
                .and_then(|entry| entry.extensions.first())
                .ok_or_else(|| {
                    GraphScanError::UnsupportedLanguage(format!(
                        "languageGlobs key {:?} must select one registered grammar",
                        rule.language
                    ))
                })?;
            let matcher =
                crate::search::glob::set(root, [rule.glob.as_str()].into_iter(), "languageGlobs")
                    .map_err(|error| GraphScanError::InvalidGlob(error.reason))?
                    .unwrap_or_else(Gitignore::empty);
            Ok(((*extension).to_owned(), matcher))
        })
        .collect()
}

fn skipped<T>(relative_path: String, code: &str, message: &str) -> GraphFactsScanOutcome<T> {
    GraphFactsScanOutcome::Skipped(GraphFactsScanDiagnostic {
        relative_path,
        code: code.to_owned(),
        message: message.to_owned(),
    })
}

/// Graph facts of one parsed file.
struct ParsedFacts {
    facts: GraphFactsDocument,
    reference_counts: Vec<crate::types::GraphReferenceCount>,
    /// Estimated heap bytes of the facts, the cache weight.
    weight: u32,
}

/// Estimated heap bytes of one file's facts: every row's own size plus its
/// strings. A file's facts outweigh its source several times (ids, ranges,
/// edge and call rows), so the source size would let the cache hold far more
/// than its capacity names.
fn facts_heap_bytes(
    facts: &GraphFactsDocument,
    reference_counts: &[crate::types::GraphReferenceCount],
) -> usize {
    use std::mem::size_of;
    let text = |value: &str| value.len();
    let option = |value: &Option<String>| value.as_deref().map_or(0, str::len);
    let list = |values: &[String]| {
        values
            .iter()
            .map(|value| size_of::<String>() + value.len())
            .sum::<usize>()
    };
    let option_list = |values: &Option<Vec<String>>| values.as_deref().map_or(0, list);
    let mut bytes =
        text(&facts.kind) + text(&facts.source) + text(&facts.language) + text(&facts.file);
    bytes += facts
        .declarations
        .iter()
        .map(|d| {
            size_of::<GraphFactDeclaration>()
                + text(&d.id)
                + text(&d.name)
                + text(&d.kind)
                + list(&d.exported_as)
                + option(&d.parent)
        })
        .sum::<usize>();
    bytes += facts
        .imports
        .iter()
        .map(|i| {
            size_of::<GraphFactImport>()
                + text(&i.id)
                + text(&i.specifier)
                + text(&i.import_kind)
                + option(&i.local_name)
                + option(&i.imported_name)
                + option(&i.resolution_hint)
                + option_list(&i.module_scope)
        })
        .sum::<usize>();
    bytes += facts
        .exports
        .iter()
        .map(|e| {
            size_of::<GraphFactExport>()
                + text(&e.id)
                + text(&e.name)
                + text(&e.export_kind)
                + option(&e.local_name)
                + option(&e.source)
        })
        .sum::<usize>();
    bytes += facts
        .calls
        .iter()
        .map(|c| {
            size_of::<GraphFactCall>()
                + text(&c.id)
                + text(&c.caller)
                + option(&c.caller_id)
                + text(&c.callee)
                + text(&c.kind)
                + option(&c.receiver_type)
        })
        .sum::<usize>();
    bytes += facts
        .common_js
        .iter()
        .map(|c| {
            size_of::<GraphFactCommonJs>()
                + option(&c.specifier)
                + text(&c.kind)
                + text(&c.binding)
                + option(&c.reason)
        })
        .sum::<usize>();
    bytes += facts
        .edges
        .iter()
        .map(|e| {
            size_of::<GraphFactEdge>()
                + text(&e.id)
                + text(&e.from)
                + text(&e.to)
                + text(&e.relation)
                + text(&e.source)
                + text(&e.resolution)
        })
        .sum::<usize>();
    bytes += facts
        .modules
        .iter()
        .map(|m| {
            size_of::<GraphFactRustModule>() + text(&m.name) + list(&m.scope) + option(&m.path)
        })
        .sum::<usize>();
    bytes += facts
        .paths
        .iter()
        .map(|p| size_of::<GraphFactPath>() + text(&p.path) + option(&p.caller_id))
        .sum::<usize>();
    bytes += facts
        .diagnostics
        .iter()
        .map(|d| size_of::<crate::graph::GraphFactDiagnostic>() + text(&d.message))
        .sum::<usize>();
    bytes += facts.error_lines.len() * size_of::<[u32; 2]>();
    bytes += reference_counts
        .iter()
        .map(|r| size_of::<crate::types::GraphReferenceCount>() + text(&r.declaration_id))
        .sum::<usize>();
    bytes
}

/// Parsed facts keyed by (content digest, relative path, parser override):
/// a rescan re-reads and hashes every file but re-parses only changed ones.
/// Content-addressed, so an edited file can never hit a stale entry.
static FACTS_MEMO: std::sync::LazyLock<moka::sync::Cache<String, Arc<ParsedFacts>>> =
    std::sync::LazyLock::new(|| {
        moka::sync::Cache::builder()
            .max_capacity(FACTS_MEMO_BYTES)
            .weigher(|_: &String, parsed: &Arc<ParsedFacts>| parsed.weight)
            .time_to_idle(std::time::Duration::from_secs(1800))
            .build()
    });

/// Estimated heap bytes the parsed-facts cache may hold.
const FACTS_MEMO_BYTES: u64 = 128 * 1024 * 1024;

/// Where a scan takes each file's facts from.
#[derive(Clone, Copy)]
pub enum FactsSource<'a> {
    /// Full graph facts (declarations, calls, paths, references), parsed
    /// once per content version and kept in the scan's own memo.
    Graph,
    /// Declarations only, from the caller's [`DeclarationsExtraction`] and
    /// its cache. The scan keeps no copy, so one cache holds a file's
    /// declarations; the view's reference counts are empty.
    Declarations(&'a DeclarationsExtraction),
}

/// A caller's declarations extraction: `(content, absolute path, parser
/// override)` to the file's declarations facts (`None`: no extractor, a
/// timeout).
pub type DeclarationsExtraction =
    dyn Fn(&str, &Path, Option<&str>) -> Option<Arc<GraphFactsDocument>> + Sync;

/// The candidate files a graph-facts scan of `options` reads, in walk order:
/// files of an outlined grammar (or mapped by `languageGlobs`) that `admit`
/// passes, up to `max_files` (unset: the walk budget). A caller that needs
/// the file list first (to count or fingerprint a scope) walks once with
/// this and hands the walk to [`scan_walked_graph_facts`].
pub fn walk_graph_fact_files(
    options: &GraphFactsScanOptions,
    admit: &(dyn Fn(&Path, Option<fs::FileType>) -> bool + Sync),
    stop: &Stop<'_>,
) -> Result<crate::types::FileSystemQueryResult, GraphScanError> {
    let root = std::path::PathBuf::from(&options.path);
    let overrides = parser_overrides(&root, options.language_globs.as_deref())?;
    let max_files = options
        .max_files
        .unwrap_or(u32::try_from(crate::scan_budget::walk_ceiling()).unwrap_or(u32::MAX));
    let extensions = crate::signatures::graph_facts::graph_fact_extensions();
    // A file only `languageGlobs` map to a grammar has an extension no grammar
    // claims, so with overrides the walk admits it by glob as well.
    let admitted = crate::search::fs_query::normalize_extensions(extensions.clone());
    let walk_filter = |path: &Path, file_type: Option<fs::FileType>, _| -> bool {
        if !admit(path, file_type) {
            return false;
        }
        let is_file = file_type.map_or_else(|| path.is_file(), |kind| kind.is_file());
        if overrides.is_empty() || !is_file {
            return true;
        }
        let name = path.file_name().map(|name| name.to_string_lossy());
        name.is_some_and(|name| crate::search::fs_query::has_extension(&name, &admitted))
            || overrides
                .iter()
                .any(|(_, matcher)| crate::search::glob::matches(matcher, path, false))
    };
    crate::search::fs_query::query_file_system(
        FileSystemQueryOptions {
            path: options.path.clone(),
            recursive: Some(true),
            ignore: options.ignore,
            entry_type: Some(crate::types::EntryType::File),
            extensions: overrides.is_empty().then_some(extensions),
            exclude_dir: options.exclude_dir.clone(),
            default_exclude_dir: options.default_exclude_dir.clone(),
            exclude: options.exclude.clone(),
            stop_at_limit: Some(true),
            limit: Some(max_files),
            ..Default::default()
        },
        &walk_filter,
        stop,
    )
    .map_err(|error| match error.kind() {
        ErrorKind::InvalidGlob => GraphScanError::InvalidGlob(error.reason),
        _ => GraphScanError::Failed(error.reason),
    })
}

/// Scans the graph facts of `walked`, a [`walk_graph_fact_files`] walk of
/// these same `options` (so `max_files` and the filters agree), handing each
/// parsed file to `map` as soon as it is parsed and keeping only what `map`
/// returns, so the scan holds what the caller needs rather than every file's
/// facts at once. A file whose source `keep` rejects (a byte prefilter, e.g.
/// a name the caller looks for) is read once and neither parsed nor mapped.
/// `stop` cancels the scan.
///
/// An unset `max_files` caps nothing but the time (see
/// [`crate::scan_budget`]): files are read in chunks and the scan stops at the
/// first chunk boundary past the budget, reporting `scan_resume`.
pub fn scan_walked_graph_facts<T: Send>(
    options: GraphFactsScanOptions,
    walked: crate::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        evaluate_walk(
            options,
            walked,
            source,
            keep,
            map,
            stop,
            Some(crate::scan_budget::SCAN_TIME_BUDGET),
        )
    }))
    .unwrap_or_else(|_| {
        Err(GraphScanError::Failed(
            "graph-facts scan failed on pathological input".to_owned(),
        ))
    })
}

/// Read, parse and map the walked candidates of a scan, within its time
/// budget when `max_files` is unset.
fn evaluate_walk<T: Send>(
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
        let metadata = source.metadata;
        let Ok(content) = String::from_utf8(source.bytes) else {
            return outcome(
                "graph.scan.readFailed",
                "file could not be read as UTF-8 text",
            );
        };
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
        let content_digest = crate::digest::sha256(content.as_bytes());
        // A file written while it was read is not settled.
        let source_stamp = Some(&metadata)
            .filter(|meta| meta.len() == content.len() as u64)
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

#[cfg(test)]
mod tests;
