//! One declarations parse per source version, shared by every reader of a
//! file's declarations (outline pages, block reads, search hits, LSP
//! narrowing) and kept typed: a reader never re-parses JSON. The rendered
//! symbols view is kept here too. Authorization and source decoding happen
//! in the caller on every request; the key uses the decoded content,
//! canonical path and the parser extension actually used (none for the
//! file's own grammar), rather than mutable timestamps.

use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use octocode_engine::graph::GraphFactsDocument;
use octocode_io::flight::KeyedFlight;
use std::sync::{Arc, OnceLock};

static CACHE: OnceLock<Store<GraphFactsDocument>> = OnceLock::new();

/// One parse per key at a time: rows reading one file concurrently wait for
/// the first parse and take its outcome (a failure too) from the slot,
/// instead of parsing the file once each.
static FLIGHT: KeyedFlight<CacheKey, Option<Option<Arc<GraphFactsDocument>>>> =
    KeyedFlight::transient();

fn cache() -> &'static Store<GraphFactsDocument> {
    CACHE.get_or_init(|| {
        Store::new(
            CacheConfig {
                max_entries: 128,
                max_bytes: 32 * 1024 * 1024,
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
    // A parser extension never contains `/`, so `auto` cannot collide with one.
    let parser = parser.map_or_else(|| "auto".to_owned(), |extension| format!("ext/{extension}"));
    let key = CacheKey {
        namespace: "ast-declarations".into(),
        scope: format!("{parser}\0{canonical_path}"),
        resource: octocode_engine::digest::sha256(source.as_bytes()),
    };
    if let Some(hit) = cache().get(&key) {
        return Some(hit.value);
    }
    let ticket = FLIGHT.join(key.clone());
    // Declarations parse in blocking row workers, never on an async worker.
    let mut slot = ticket.blocking_turn();
    if let Some(outcome) = slot.as_ref() {
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
        let bytes = weight(&document) + canonical_path.len() + key.resource.len();
        // Keyed by the source digest, so an entry never goes stale. The
        // stored value is the one returned: no copy, no second lookup.
        cache().put(key, document, bytes, CacheClass::Immutable)
    });
    *slot = Some(outcome.clone());
    outcome
}

/// A parse's cache weight, estimated from what it holds (never by
/// serializing it): a fixed cost per fact plus its free text.
fn weight(document: &GraphFactsDocument) -> usize {
    const DECLARATION: usize = 256;
    const FACT: usize = 128;
    let facts = document.imports.len()
        + document.exports.len()
        + document.calls.len()
        + document.common_js.len()
        + document.edges.len()
        + document.modules.len()
        + document.paths.len();
    512 + document.declarations.len() * DECLARATION
        + facts * FACT
        + document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.len())
            .sum::<usize>()
        + document.error_lines.len() * 8
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
const SYMBOLS_NAMESPACE: &str = "symbols-view/v2";

static SYMBOLS: OnceLock<Store<SymbolsView>> = OnceLock::new();
static SYMBOLS_DISK: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Keep rendered symbols views in `dir` too, across processes. The runtime
/// calls it once, before any read, when storage is persistent; a later
/// call (or one after the first view) changes nothing.
pub(crate) fn persist_symbols_views(dir: std::path::PathBuf) {
    let _ = SYMBOLS_DISK.set(dir);
}

/// The process's symbols views: in memory, and on disk once
/// [`persist_symbols_views`] named a directory.
pub(crate) fn symbols_store() -> &'static Store<SymbolsView> {
    SYMBOLS.get_or_init(|| {
        Store::new(
            CacheConfig {
                max_entries: 64,
                max_bytes: 32 * 1024 * 1024,
                disk_bytes: 256 * 1024 * 1024,
            },
            SYMBOLS_DISK.get().cloned(),
        )
    })
}

/// The `minify:"symbols"` view of `source` at `path` in `store`: rendered
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
mod tests {
    use super::{all_declarations, document, symbols_store, symbols_view};
    use octocode_engine::graph::GraphFactsDocument;
    use octocode_engine::signatures::OutlineUnavailable;

    /// One render per source version, a missing outline included.
    #[test]
    fn symbols_views_and_their_verdicts_render_once_per_version() {
        let calls = Cell::new(0);
        let path = format!("symbols-cache-test-{}", std::process::id());
        let render = |outline| {
            calls.set(calls.get() + 1);
            Ok(super::SymbolsView {
                outline,
                warning: None,
                redacted_lines: Vec::new(),
            })
        };
        let first = symbols_view(symbols_store(), "fn a() {}", &path, || {
            render(Ok("1\tfn a()".into()))
        });
        let again = symbols_view(symbols_store(), "fn a() {}", &path, || {
            render(Ok("other".into()))
        });
        assert!(std::sync::Arc::ptr_eq(&first, &again));
        let missing = || render(Err(OutlineUnavailable::NotSmaller));
        assert_eq!(
            symbols_view(symbols_store(), "x", &path, missing).outline,
            Err(OutlineUnavailable::NotSmaller)
        );
        assert_eq!(
            symbols_view(symbols_store(), "x", &path, missing).outline,
            Err(OutlineUnavailable::NotSmaller)
        );
        assert_eq!(calls.get(), 2);
    }

    /// A view stored on disk is read back by a fresh store (another
    /// process): the outline renders once across processes.
    #[test]
    fn symbols_views_persist_across_stores() {
        let dir = tempfile::tempdir().expect("dir");
        let store = || {
            crate::cache::Store::new(
                crate::cache::CacheConfig::default(),
                Some(dir.path().to_owned()),
            )
        };
        let calls = Cell::new(0);
        let render = || {
            calls.set(calls.get() + 1);
            Ok(super::SymbolsView {
                outline: Ok("1\tfn a()\n".into()),
                warning: Some("recovered".into()),
                redacted_lines: vec![(1, 1)],
            })
        };
        let first = super::symbols_view(&store(), "fn a() {}", "a.rs", render);
        let again = super::symbols_view(&store(), "fn a() {}", "a.rs", render);
        assert_eq!(first, again);
        assert_eq!(calls.get(), 1);
    }
    use std::cell::Cell;

    /// A parse named by `file`.
    fn named(file: &str) -> Option<GraphFactsDocument> {
        Some(GraphFactsDocument {
            file: file.to_owned(),
            ..GraphFactsDocument::default()
        })
    }

    #[test]
    fn content_path_and_parser_identity_control_reuse() {
        let calls = Cell::new(0);
        let parse = || {
            calls.set(calls.get() + 1);
            named("declarations")
        };
        let path = format!("cache-test-{}", std::process::id());
        let first = document("fn one() {}", &path, None, parse).unwrap();
        let second = document("fn one() {}", &path, None, parse).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(calls.get(), 1);
        document("fn two() {}", &path, None, parse).unwrap();
        document("fn one() {}", &format!("{path}-other"), None, parse).unwrap();
        document("fn one() {}", &path, Some("cpp"), parse).unwrap();
        assert_eq!(calls.get(), 4);
    }

    /// Each parser override keys its own entry: the same override reuses
    /// it, another one parses again.
    #[test]
    fn every_parser_override_keys_its_own_entry() {
        let calls = Cell::new(0);
        let parse = || {
            calls.set(calls.get() + 1);
            named(&format!("declarations{}", calls.get()))
        };
        let path = format!("override-cache-test-{}", std::process::id());
        let py = document("x = 1", &path, Some("py"), parse).unwrap();
        let again = document("x = 1", &path, Some("py"), parse).unwrap();
        assert!(std::sync::Arc::ptr_eq(&py, &again));
        assert_eq!(calls.get(), 1);
        let rb = document("x = 1", &path, Some("rb"), parse).unwrap();
        assert_eq!(rb.file, "declarations2");
        let own = document("x = 1", &path, None, parse).unwrap();
        assert_eq!(own.file, "declarations3");
        let cpp = document("x = 1", &path, Some("cpp"), parse).unwrap();
        let again = document("x = 1", &path, Some("cpp"), parse).unwrap();
        assert!(std::sync::Arc::ptr_eq(&cpp, &again));
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn failed_extraction_is_not_cached() {
        let path = format!("failed-cache-test-{}", std::process::id());
        assert!(document("source", &path, None, || None).is_none());
        assert_eq!(
            document("source", &path, None, || named("retry")).map(|parse| parse.file.clone()),
            Some("retry".to_owned())
        );
    }

    /// Rows reading one file at once parse it once: the others wait for the
    /// first parse and share it (a failed one too, within its wave).
    #[test]
    fn concurrent_reads_of_one_version_parse_it_once() {
        let path = format!("flight-cache-test-{}", std::process::id());
        let parses = std::sync::atomic::AtomicUsize::new(0);
        let gate = std::sync::Barrier::new(8);
        let parse = || {
            parses.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            named("shared")
        };
        let outcomes = std::thread::scope(|scope| {
            let workers = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        gate.wait();
                        document("fn shared() {}", &path, None, parse)
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| worker.join().expect("worker"))
                .collect::<Vec<_>>()
        });
        assert_eq!(parses.load(std::sync::atomic::Ordering::SeqCst), 1);
        let first = outcomes[0].as_ref().expect("parse");
        assert!(outcomes.iter().all(|outcome| {
            outcome
                .as_ref()
                .is_some_and(|parse| std::sync::Arc::ptr_eq(parse, first))
        }));
        // A failure is shared by its wave only: the next read parses again.
        let failing = format!("{path}-failing");
        let failures = std::sync::atomic::AtomicUsize::new(0);
        let fail = || {
            failures.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            None
        };
        let gate = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            let workers = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        gate.wait();
                        document("x", &failing, None, fail)
                    })
                })
                .collect::<Vec<_>>();
            for worker in workers {
                assert!(worker.join().expect("worker").is_none());
            }
        });
        assert!(failures.load(std::sync::atomic::Ordering::SeqCst) < 4);
        assert!(document("x", &failing, None, || named("later")).is_some());
    }

    /// A reader on a thread inside a tokio runtime (a gh tool's `block_on`,
    /// an entered handle; current-thread or multi-thread) takes its turn
    /// without panicking, and shares the one parse.
    #[test]
    fn readers_inside_a_runtime_take_their_turn() {
        let path = format!("runtime-cache-test-{}.rs", std::process::id());
        let read = || super::file_document("fn inside() {}\n", &path);
        let current = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        assert!(current.block_on(async { read() }).is_some());
        let multi = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .expect("runtime");
        assert!(multi.block_on(async { read() }).is_some());
        let _entered = multi.enter();
        assert!(read().is_some());
    }

    /// The typed declarations come from the one cached parse, and that
    /// parse is the engine's own declarations document.
    #[test]
    fn typed_declarations_and_the_engine_document_share_one_parse() {
        let source = "/// Doc.\nfn first() {\n    1;\n}\nfn second() {}\n";
        let path = format!("typed-cache-test-{}.rs", std::process::id());
        let declared = all_declarations(source, &path).expect("declarations");
        let spans = declared
            .iter()
            .map(|d| (d.name.as_str(), d.doc_start, d.line, d.start, d.end))
            .collect::<Vec<_>>();
        assert_eq!(
            spans,
            [("first", Some(1), 2, 2, 4), ("second", None, 5, 5, 5)]
        );
        let cached =
            document(source, &path, None, || panic!("the parse is cached")).expect("cached");
        assert_eq!(
            *cached,
            octocode_engine::signatures::extract_declarations_document(source, &path)
                .expect("engine")
        );
    }
}
