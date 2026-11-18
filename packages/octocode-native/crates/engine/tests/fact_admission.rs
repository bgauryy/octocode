//! Graph-fact admission of a language that has a fact provider and no parser,
//! through the fixture TOML grammar (`octocode_test_support::conformance`):
//! the same walk, scan and bound fact mapper as a parsed language, and the
//! caller's checkpoints validated before the memo.
use octocode_engine::fact_scan::{
    FactsSource, GraphFactsReuseContext, scan_walked_graph_facts_with_registry,
    walk_graph_fact_files_with_registry,
};
use octocode_engine::graph::GraphFactsTypedEntry;
use octocode_engine::signatures::languages::RegistryView;
use octocode_engine::text::analysis::*;
use octocode_engine::types::GraphFactsScanOptions;
use octocode_test_support::conformance::toml_registry;
use std::fs;
use std::path::Path;

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn provider_only_toml_uses_the_same_walk_scan_and_bound_fact_mapper() {
    let fixture = tempfile::tempdir().unwrap();
    let text = "\"λ\" = 1\nhandler = 'not a function'\n";
    fs::write(fixture.path().join("a.toml"), text).unwrap();
    fs::write(fixture.path().join("broken.toml"), "x = [").unwrap();
    let options = GraphFactsScanOptions {
        path: path_string(fixture.path()),
        max_files: Some(10),
        ..Default::default()
    };
    let registry = toml_registry();
    let capability = registry
        .capabilities()
        .into_iter()
        .find(|row| row.language == "TOML conformance")
        .unwrap();
    assert!(capability.facts.declarations && capability.graph_facts);
    assert!(
        !capability.facts.calls
            && !capability.structural_search
            && !capability.syntax_tree
            && !capability.signature_outline
    );
    assert!(
        octocode_engine::signatures::languages::RegistryView::installed()
            .find("toml")
            .is_none()
    );
    let walked =
        walk_graph_fact_files_with_registry(&options, &|_, _| true, &|| false, registry).unwrap();
    assert_eq!(walked.entries.len(), 2);
    let output = scan_walked_graph_facts_with_registry(
        options,
        walked,
        FactsSource::Graph,
        &|_| true,
        &|view| {
            let mut budget = MapBudget::new(
                MapLimits::default(),
                std::time::Instant::now() + std::time::Duration::from_secs(10),
                &|| false,
            );
            assert_eq!(view.facts.declarations.len(), 2);
            assert!(view.facts.calls.is_empty());
            for declaration in &view.facts.declarations {
                let span = declaration.selection_byte_range.unwrap();
                let projection = view.source.project(span, &mut budget).unwrap();
                let descriptors = view
                    .source
                    .descriptors(std::slice::from_ref(&projection), &mut budget)
                    .unwrap();
                SourceCatalog::new(&descriptors, &mut budget)
                    .unwrap()
                    .validate_projection(&projection, span.range, &mut budget)
                    .unwrap();
                assert!(matches!(
                    projection.segments[0].branches[0],
                    OriginBranch::Copied(_)
                ));
            }
            Some(view.facts.clone())
        },
        &|| false,
        registry,
        MapLimits::default(),
        None,
    )
    .unwrap();
    assert_eq!(output.kept.len(), 1);
    assert_eq!(output.skipped.len(), 1);
    assert_eq!(output.skipped[0].code, "graph.scan.extractFailed");
}

#[expect(
    clippy::unwrap_used,
    reason = "a test helper: a scan that fails fails the test that asked for it"
)]
fn scan_with_source(
    options: &GraphFactsScanOptions,
    walked: &octocode_engine::types::FileSystemQueryResult,
    source: FactsSource<'_>,
    registry: RegistryView<'_>,
) -> (GraphFactsTypedEntry, bool) {
    scan_walked_graph_facts_with_registry(
        options.clone(),
        walked.clone(),
        source,
        &|_| true,
        &|view| Some((view.to_entry(), view.reused)),
        &|| false,
        registry,
        MapLimits::default(),
        None,
    )
    .unwrap()
    .kept
    .remove(0)
    .1
}

#[test]
fn caller_checkpoints_are_validated_before_memo_for_parsers_and_providers() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for (file, text) in [
        ("a.toml", "answer = 42\n"),
        ("a.rs", "fn checkpointed() {}\n"),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        fs::write(fixture.path().join(file), text).unwrap();
        let options = GraphFactsScanOptions {
            path: path_string(fixture.path()),
            max_files: Some(10),
            ..Default::default()
        };
        let registry = toml_registry();
        let walked =
            walk_graph_fact_files_with_registry(&options, &|_, _| true, &|| false, registry)
                .unwrap();
        let (entry, _) = scan_with_source(&options, &walked, FactsSource::Graph, registry);
        let visited = AtomicBool::new(false);
        let reuse = |_: &GraphFactsReuseContext<'_>| {
            visited.store(true, Ordering::SeqCst);
            Some(entry.clone())
        };
        let (cached, reused) = scan_with_source(
            &options,
            &walked,
            FactsSource::CachedGraph(&reuse),
            registry,
        );
        assert!(
            visited.load(Ordering::SeqCst),
            "{file}: caller checkpoint bypassed by memo"
        );
        assert!(reused);
        assert_eq!(cached.facts, entry.facts);
        for invalid in 0..4 {
            let mut wrong = entry.clone();
            match invalid {
                0 => wrong.content_digest = "wrong".into(),
                1 => wrong.facts.source_digest = "wrong".into(),
                2 => wrong.facts.schema_version = 0,
                _ => wrong.facts.file = "other.rs".into(),
            }
            let reuse = |_: &GraphFactsReuseContext<'_>| Some(wrong.clone());
            let (actual, _) = scan_with_source(
                &options,
                &walked,
                FactsSource::CachedGraph(&reuse),
                registry,
            );
            assert_eq!(
                actual.facts, entry.facts,
                "{file}: invalid checkpoint accepted"
            );
        }
    }
}
