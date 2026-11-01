//! One parse per source version, shared by every reader of a file's
//! syntax facts and kept typed: a reader never re-parses JSON. Two parses
//! share the store and its single flight: the declarations parse (outline
//! pages, block reads, search hits, LSP narrowing) and the full graph facts
//! (CommonJS loads and heritage edges, read by lspSearch importer recovery
//! and type-hierarchy fallbacks). The rendered symbols view is kept here
//! too. Authorization and source decoding happen in the caller on every
//! request; the key uses the parse kind, decoded content, canonical path and
//! the parser extension actually used (none for the file's own grammar),
//! rather than mutable timestamps.

use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use octocode_engine::graph::GraphFactsDocument;
use octocode_io::flight::KeyedFlight;
use std::sync::{Arc, OnceLock};

static CACHE: OnceLock<Store<GraphFactsDocument>> = OnceLock::new();

/// One parse per key at a time: rows reading one file concurrently wait for
/// the first parse and take its outcome (a failure too) from the slot,
/// instead of parsing the file once each.
static FLIGHT: KeyedFlight<CacheKey, Wave> = KeyedFlight::transient();

/// What a key's slot holds during one concurrent wave.
#[derive(Default)]
enum Wave {
    /// No row of the wave has parsed the source yet.
    #[default]
    Unparsed,
    /// The wave's parse: the document, or `None` when extraction failed.
    Parsed(Option<Arc<GraphFactsDocument>>),
}

fn cache() -> &'static Store<GraphFactsDocument> {
    CACHE.get_or_init(|| {
        Store::new(
            CacheConfig {
                max_entries: 384,
                max_bytes: 96 * 1024 * 1024,
                ..CacheConfig::default()
            },
            None,
        )
    })
}

/// The declarations parse of `source` at `canonical_path`, typed, once per
/// source version. `parser` is the extension whose grammar parses the
/// source; `None` is the file's own. Every override keeps its own entry.
/// `extraction` returns the parse; `None` (no extractor, a timeout) is never
/// kept as an outline.
pub(crate) fn document(
    source: &str,
    canonical_path: &str,
    parser: Option<&str>,
    extraction: impl FnOnce() -> Option<GraphFactsDocument>,
) -> Option<Arc<GraphFactsDocument>> {
    parse_once(
        key_of(DECLARATIONS, source, canonical_path, parser),
        canonical_path,
        extraction,
    )
}

/// The full graph facts of `content` at `path` (its own grammar): what the
/// declarations parse leaves out, CommonJS loads and heritage edges, once
/// per source version; `None` when no extractor parses it (never kept).
pub(crate) fn graph_facts(content: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    parse_once(key_of(GRAPH_FACTS, content, path, None), path, || {
        octocode_engine::signatures::extract_graph_facts(content, path)
    })
}

/// Cache namespaces: one per parse kind.
const DECLARATIONS: &str = "ast-declarations";
const GRAPH_FACTS: &str = "graph-facts";

/// The cached parse under `key`, or `extraction`'s, parsed by one reader of
/// a concurrent wave (see [`FLIGHT`]).
fn parse_once(
    key: CacheKey,
    canonical_path: &str,
    extraction: impl FnOnce() -> Option<GraphFactsDocument>,
) -> Option<Arc<GraphFactsDocument>> {
    if let Some(hit) = cache().get(&key) {
        return Some(hit.value);
    }
    let ticket = FLIGHT.join(key.clone());
    // Declarations parse in blocking row workers, never on an async worker.
    let mut slot = ticket.blocking_turn();
    if let Wave::Parsed(outcome) = &*slot {
        return outcome.clone();
    }
    // The previous wave's parse may have landed between the lookup and the
    // join.
    if let Some(hit) = cache().get(&key) {
        return Some(hit.value);
    }
    // Extraction can block or time out: never hold the cache lock while
    // parsing (only this key's turn), and never retain a failed extraction
    // past its wave.
    let outcome = extraction().map(|document| {
        #[cfg(test)]
        PARSED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.clone());
        let bytes = weight(&document) + canonical_path.len() + key.resource.len();
        // Keyed by the source digest, so an entry never goes stale. The
        // stored value is the one returned: no copy, no second lookup.
        cache().put(key, document, bytes, CacheClass::Immutable)
    });
    *slot = Wave::Parsed(outcome.clone());
    outcome
}

/// Every key a parse went through the cache under. The cache itself is
/// bounded and shared by every test thread, so residency proves nothing
/// under parallel load; this record does.
#[cfg(test)]
static PARSED: std::sync::Mutex<std::collections::BTreeSet<CacheKey>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Whether `source` at `canonical_path` was parsed through [`document`]
/// with `parser` (`None`: the file's own grammar).
#[cfg(test)]
pub(crate) fn parsed_through_cache(
    source: &str,
    canonical_path: &str,
    parser: Option<&str>,
) -> bool {
    PARSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&key_of(DECLARATIONS, source, canonical_path, parser))
}

/// Whether `source` at `path` was parsed through [`graph_facts`].
#[cfg(test)]
pub(crate) fn facts_parsed_through_cache(source: &str, path: &str) -> bool {
    PARSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&key_of(GRAPH_FACTS, source, path, None))
}

/// The cache key of `source` at `canonical_path` parsed as `namespace` with
/// `parser`.
fn key_of(namespace: &str, source: &str, canonical_path: &str, parser: Option<&str>) -> CacheKey {
    // A parser extension never contains `/`, so `auto` cannot collide with one.
    let parser = parser.map_or_else(|| "auto".to_owned(), |extension| format!("ext/{extension}"));
    CacheKey {
        namespace: namespace.into(),
        scope: format!("{parser}\0{canonical_path}"),
        resource: octocode_engine::digest::sha256(source.as_bytes()),
    }
}

/// A parse's cache weight, estimated from what it holds (never by
/// serializing it): a fixed cost per fact plus its free text.
fn weight(document: &GraphFactsDocument) -> usize {
    const DECLARATION: usize = 256;
    const FACT: usize = 128;
    let facts = document.imports.len()
        + document.exports.len()
        + document.calls.len()
        + document.argument_sites.len()
        + document.common_js.len()
        + document.edges.len()
        + document.modules.len()
        + document.paths.len();
    512 + document.source_templates.heap_bytes()
        + document.declarations.len() * DECLARATION
        + facts * FACT
        + document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.len())
            .sum::<usize>()
        + (document.error_lines.len() + document.test_lines.len()) * 8
}

/// The declarations of `content` at `path` in the file's own grammar.
pub(crate) fn file_document(content: &str, path: &str) -> Option<Arc<GraphFactsDocument>> {
    document(content, path, None, || {
        octocode_engine::signatures::extract_declarations_document(content, path)
    })
}

/// One declaration of the engine outline, 1-based lines.
pub(crate) struct Declared {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) doc_start: Option<usize>,
    /// Its name line.
    pub(crate) line: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Declared {
    fn of(declaration: &octocode_engine::graph::GraphFactDeclaration) -> Self {
        let line = |zero_based: u32| zero_based as usize + 1;
        Self {
            name: declaration.name.clone(),
            kind: declaration.kind.clone(),
            doc_start: declaration.doc_line.map(line),
            line: line(declaration.selection_range.start.line),
            start: line(declaration.range.start.line),
            end: line(declaration.range.end.line),
        }
    }
}

/// Every declaration the engine outlines, one-line ones included, in
/// outline order; `None` when the file type has no outline.
pub(crate) fn all_declarations(content: &str, path: &str) -> Option<Vec<Declared>> {
    // Shared with outline pages and search hits: one parse per file version.
    let document = file_document(content, path)?;
    Some(document.declarations.iter().map(Declared::of).collect())
}

/// The symbols view of one source version: the outline, or why it has
/// none, and the parse-recovery note the outline carries. Every field is
/// rendered from the source after secret redaction, so neither the memory
/// nor the disk tier ever holds a secret the response would redact.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SymbolsView {
    pub(crate) outline: Result<String, octocode_engine::signatures::OutlineUnavailable>,
    /// Why the outline may miss declarations (a recovered parse).
    pub(crate) warning: Option<String>,
    /// The source lines (1-based, ascending) the redaction changed, with
    /// the placeholders each gained: a page states how many it shows.
    pub(crate) redacted_lines: Vec<(usize, usize)>,
}

/// Rendered views outlive one process under persistent storage: a CLI
/// paging an outline renders it once, not once per page. Bump the version
/// when the rendered view changes shape; a release changes the scope.
/// v2: views render from redacted text (v1 files held raw secrets and are
/// never read again).
/// v3: corrected Markdown headings and complete YAML block-scalar spans.
const SYMBOLS_NAMESPACE: &str = "symbols-view/v3";

/// A runtime's rendered views, with its own optional persistence directory.
pub(crate) fn symbols_store(dir: Option<std::path::PathBuf>) -> Store<SymbolsView> {
    Store::new(
        CacheConfig {
            max_entries: 64,
            max_bytes: 32 * 1024 * 1024,
            disk_bytes: 256 * 1024 * 1024,
        },
        dir,
    )
}

/// The `view:"symbols"` view of `source` at `path` in `store`: rendered
/// once per source version and reused by every outline page; a hit costs
/// no render and no redaction. A missing outline is a verdict of the same
/// version, so it is kept too. `render` builds the view from redacted text
/// (`Ok`: kept, on disk too), or, when the source could not be redacted,
/// for this page only (`Err`: never kept).
pub(crate) fn symbols_view(
    store: &Store<SymbolsView>,
    source: &str,
    path: &str,
    render: impl FnOnce() -> Result<SymbolsView, SymbolsView>,
) -> Arc<SymbolsView> {
    let key = CacheKey {
        namespace: SYMBOLS_NAMESPACE.into(),
        scope: format!("{}\0{path}", env!("CARGO_PKG_VERSION")),
        resource: octocode_engine::digest::sha256(source.as_bytes()),
    };
    if let Some(hit) = store.get(&key) {
        return hit.value;
    }
    let view = match render() {
        Ok(view) => view,
        Err(page_only) => return Arc::new(page_only),
    };
    let bytes = view.outline.as_ref().map_or(0, String::len)
        + view.warning.as_ref().map_or(0, String::len)
        + view.redacted_lines.len() * 2 * std::mem::size_of::<usize>()
        + path.len()
        + key.resource.len();
    store.put(key, view, bytes, CacheClass::Immutable)
}

#[cfg(test)]
mod tests;
