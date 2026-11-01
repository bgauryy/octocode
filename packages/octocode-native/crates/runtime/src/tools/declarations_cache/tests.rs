use super::{
    all_declarations, document, facts_parsed_through_cache, file_document, graph_facts,
    symbols_store, symbols_view,
};
use octocode_engine::graph::GraphFactsDocument;
use octocode_engine::signatures::OutlineUnavailable;
use std::cell::Cell;

/// One render per source version, a missing outline included.
#[test]
fn symbols_views_and_their_verdicts_render_once_per_version() {
    let store = symbols_store(None);
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
    let first = symbols_view(&store, "fn a() {}", &path, || {
        render(Ok("1\tfn a()".into()))
    });
    let again = symbols_view(&store, "fn a() {}", &path, || render(Ok("other".into())));
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    let missing = || render(Err(OutlineUnavailable::NotSmaller));
    assert_eq!(
        symbols_view(&store, "x", &path, missing).outline,
        Err(OutlineUnavailable::NotSmaller)
    );
    assert_eq!(
        symbols_view(&store, "x", &path, missing).outline,
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

#[test]
fn corrected_symbols_views_ignore_persisted_old_renderer_entries() {
    use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};

    let dir = tempfile::tempdir().expect("cache directory");
    let store = || Store::new(CacheConfig::default(), Some(dir.path().to_owned()));
    let source = "# C#\nBody\n";
    let path = "README.md";
    store().put(
        CacheKey {
            namespace: "symbols-view/v2".into(),
            scope: format!("{}\0{path}", env!("CARGO_PKG_VERSION")),
            resource: octocode_engine::digest::sha256(source.as_bytes()),
        },
        super::SymbolsView {
            outline: Ok("1\t# C".into()),
            warning: None,
            redacted_lines: Vec::new(),
        },
        128,
        CacheClass::Immutable,
    );
    let render = || {
        Ok(super::SymbolsView {
            outline: Ok(crate::content::markdown_heading_outline(source, path).expect("headings")),
            warning: None,
            redacted_lines: Vec::new(),
        })
    };
    let current = symbols_view(&store(), source, path, render);
    assert_eq!(current.outline.as_deref(), Ok("1\t# C#"));
    let replay = symbols_view(&store(), source, path, || {
        panic!("reuse the corrected view")
    });
    assert_eq!(current, replay);
}

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
    let cached = document(source, &path, None, || panic!("the parse is cached")).expect("cached");
    assert_eq!(
        *cached,
        octocode_engine::signatures::extract_declarations_document(source, &path).expect("engine")
    );
}

/// The full graph facts keep their own entry beside the declarations
/// parse: a repeat read hits it, heritage edges are kept, an edit parses
/// again.
#[test]
fn one_parse_serves_every_reader_of_a_source_version() {
    let file = format!("/facts-cache-{}/a.ts", std::process::id());
    let first = graph_facts("export class A extends B {}\n", &file).expect("facts");
    let again = graph_facts("export class A extends B {}\n", &file).expect("facts");
    assert!(
        std::sync::Arc::ptr_eq(&first, &again),
        "a repeat read hits the cache"
    );
    assert!(
        first.edges.iter().any(|edge| edge.relation == "extends"),
        "heritage edges are kept"
    );
    let edited = graph_facts("export class A {}\n", &file).expect("facts");
    assert!(
        !std::sync::Arc::ptr_eq(&first, &edited),
        "an edit is another version"
    );
    assert!(!edited.edges.iter().any(|edge| edge.relation == "extends"));
    // The declarations parse of the same source is another entry.
    let declarations = file_document("export class A extends B {}\n", &file).expect("declarations");
    assert!(!std::sync::Arc::ptr_eq(&first, &declarations));
}

/// Concurrent readers of one source version's graph facts share one parse
/// (the single flight): every reader gets the same stored document.
#[test]
fn concurrent_graph_facts_readers_parse_once() {
    let file = format!("/facts-flight-{}/a.ts", std::process::id());
    let source = "export class Shared extends Base {}\n";
    let gate = std::sync::Barrier::new(8);
    let outcomes = std::thread::scope(|scope| {
        let workers = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    gate.wait();
                    graph_facts(source, &file)
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("worker").expect("facts"))
            .collect::<Vec<_>>()
    });
    assert!(facts_parsed_through_cache(source, &file));
    assert!(
        outcomes
            .iter()
            .all(|outcome| std::sync::Arc::ptr_eq(outcome, &outcomes[0])),
        "one parse serves the wave"
    );
}
