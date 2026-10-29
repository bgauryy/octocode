//! The graph-facts scan: walks a tree for files of an outlined grammar,
//! reads and parses each one (reusing unchanged files' facts), and hands the
//! facts to the caller's map. The graph types it fills live in
//! [`crate::graph`]; the extraction lives in [`crate::signatures`].
use std::{fs, path::Path, sync::Arc};

use ignore::gitignore::Gitignore;

use crate::error::ErrorKind;
use crate::graph::{GraphFactsDocument, GraphFactsTypedEntry};
use crate::search::walk::Stop;
use crate::stamp::SourceStamp;
use crate::types::{FileSystemQueryOptions, GraphFactsScanDiagnostic, GraphFactsScanOptions};

mod evaluate;
pub use evaluate::SOURCE_BUDGET_EXCEEDED;
mod memo;

use crate::signatures::languages::RegistryView;
use crate::text::analysis::{AnalysisOperation, MapLimits};
#[cfg(test)]
use evaluate::evaluate_walk;
#[cfg(test)]
use memo::facts_heap_bytes;

/// Largest source the graph-facts scan parses; a larger file stays an
/// unparsed (skipped) node. One bound for graph ingest and its reuse check.
pub const MAX_GRAPH_SOURCE_BYTES: u32 = 1_000_000;

/// One parsed file, borrowed from the scan's cache: a caller reduces it to
/// what it keeps before the next file is read.
pub struct GraphFactsView<'a> {
    pub relative_path: &'a str,
    /// The source the facts were parsed from (the one read of the file).
    pub content: &'a str,
    pub content_digest: &'a str,
    pub source_stamp: Option<SourceStamp>,
    pub facts: &'a GraphFactsDocument,
    pub admitted: &'a crate::text::analysis::AdmittedSource<'a>,
    pub source: &'a crate::text::analysis::BoundSource<'a>,
    pub reference_counts: &'a [crate::types::GraphReferenceCount],
    /// Full graph facts came from an in-memory or caller-provided cache.
    /// The declarations-only extractor owns separate cache telemetry.
    pub reused: bool,
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
    Source(crate::text::analysis::SourceError),
}

impl std::fmt::Display for GraphScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGlob(message) => write!(f, "[ast.language.invalidGlob] {message}"),
            Self::UnsupportedLanguage(message) => write!(f, "[ast.language.unsupported] {message}"),
            Self::Failed(message) => f.write_str(message),
            Self::Source(error) => error.fmt(f),
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
    parser_overrides_with_registry(root, rules, RegistryView::installed())
}

pub fn parser_overrides_with_registry(
    root: &Path,
    rules: Option<&[crate::types::GraphLanguageGlob]>,
    registry: RegistryView<'_>,
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
            let extension = registry
                .select_one(&rule.language)
                .and_then(|entry| entry.extensions().first())
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

/// Where a scan takes each file's facts from.
#[derive(Clone, Copy)]
pub enum FactsSource<'a> {
    /// Full graph facts (declarations, calls, paths, references), parsed
    /// once per content version and kept in the scan's own memo.
    Graph,
    /// Full facts may be reused from the caller's versioned persistent store,
    /// after the scan reads and fingerprints the current source.
    CachedGraph(&'a GraphFactsReuse<'a>),
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

/// The source identity a caller's persisted facts must satisfy.
pub struct GraphFactsReuseContext<'a> {
    pub relative_path: &'a str,
    pub content_digest: &'a str,
    pub source_digest: &'a str,
    pub parser: Option<&'a str>,
}

impl GraphFactsReuseContext<'_> {
    /// The scan and persistent-store accounting share this acceptance check.
    pub fn accepts(&self, entry: &GraphFactsTypedEntry) -> bool {
        entry.relative_path == self.relative_path
            && entry.content_digest == self.content_digest
            && entry.facts.file == self.relative_path
            && entry.facts.source_digest == self.source_digest
            && entry.facts.schema_version == crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
    }
}

/// The current source's identity and parser override to previous facts.
/// The caller namespaces its store by extractor version and parser policy.
/// The scan rejects mismatched path/digest records and supplies current stamps.
pub type GraphFactsReuse<'a> =
    dyn Fn(&GraphFactsReuseContext<'_>) -> Option<GraphFactsTypedEntry> + Sync + 'a;

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
    walk_graph_fact_files_with_registry(options, admit, stop, RegistryView::installed())
}

pub fn walk_graph_fact_files_with_registry(
    options: &GraphFactsScanOptions,
    admit: &(dyn Fn(&Path, Option<fs::FileType>) -> bool + Sync),
    stop: &Stop<'_>,
    registry: RegistryView<'_>,
) -> Result<crate::types::FileSystemQueryResult, GraphScanError> {
    let root = std::path::PathBuf::from(&options.path);
    let overrides =
        parser_overrides_with_registry(&root, options.language_globs.as_deref(), registry)?;
    let max_files = options
        .max_files
        .unwrap_or(u32::try_from(crate::scan_budget::walk_ceiling()).unwrap_or(u32::MAX));
    let extensions = registry.extensions(AnalysisOperation::GraphFacts);
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
    scan_walked_graph_facts_with_registry(
        options,
        walked,
        source,
        keep,
        map,
        stop,
        RegistryView::installed(),
        MapLimits::default(),
    )
}

pub fn scan_walked_graph_facts_with_registry<T: Send>(
    options: GraphFactsScanOptions,
    walked: crate::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    keep: &(dyn Fn(&str) -> bool + Sync),
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<T> + Sync),
    stop: &Stop<'_>,
    registry: RegistryView<'_>,
    map_limits: MapLimits,
) -> Result<GraphFactsMappedScan<T>, GraphScanError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        evaluate::evaluate_walk_with_registry(
            options,
            walked,
            source,
            keep,
            map,
            stop,
            Some(crate::scan_budget::SCAN_TIME_BUDGET),
            registry,
            map_limits,
        )
    }))
    .unwrap_or_else(|_| {
        Err(GraphScanError::Failed(
            "graph-facts scan failed on pathological input".to_owned(),
        ))
    })
}

#[cfg(test)]
mod tests;
