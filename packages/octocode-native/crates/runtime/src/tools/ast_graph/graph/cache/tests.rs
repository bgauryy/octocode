use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};
mod reuse;

#[test]
fn artifacts_are_scoped_by_source_path_content_and_parser_policy() {
    let dir = Fixture::new();
    let policy = workspace_policy(&dir.0);
    let security = ContentSecurity::new();
    let root = dir.0.canonicalize().unwrap();
    let scan = GraphScan {
        root: root.clone(),
        fact_cache: Some(root.join(".facts")),
        ..Default::default()
    };
    let cache =
        FactCache::new(&scan, &root, &policy, &security, RegistryView::installed()).unwrap();
    let source = "def cached_fixture():\n    return 7\n";
    let facts: GraphFactsDocument = serde_json::from_str(
        &octocode_engine::signatures::extract_graph_facts(source, "one.py").unwrap(),
    )
    .unwrap();
    let digest = octocode_engine::digest::sha256(source.as_bytes());
    let stop = || false;
    let mut budget = source_budget(&stop);
    let admitted = octocode_engine::text::analysis::AdmittedSource::from_bytes(
        source.as_bytes(),
        "one.py",
        &mut budget,
    )
    .unwrap();
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    let view = octocode_engine::fact_scan::GraphFactsView {
        admitted: &admitted,
        source: &bound,
        relative_path: "one.py",
        content: source,
        content_digest: &digest,
        source_stamp: None,
        facts: &facts,
        reference_counts: &[],
        reused: false,
    };
    cache.store(&view);
    assert_eq!(cache.load("one.py", &digest).unwrap().facts, facts);
    assert!(cache.load("two.py", &digest).is_none());
    assert!(cache.load("one.py", "different").is_none());
    let mut changed = scan.clone();
    changed
        .language_globs
        .insert("python".into(), vec!["**/*.py".into()]);
    assert!(
        FactCache::new(
            &changed,
            &root,
            &policy,
            &security,
            RegistryView::installed()
        )
        .unwrap()
        .load("one.py", &digest)
        .is_none()
    );
    std::fs::write(
        cache.disk.as_ref().unwrap().file(&cache.key("one.py")),
        "truncated artifact",
    )
    .unwrap();
    assert!(cache.load("one.py", &digest).is_none());
}

fn source_budget(
    stop: &(dyn Fn() -> bool + Sync),
) -> octocode_engine::text::analysis::MapBudget<'_> {
    use octocode_engine::text::analysis::{MapBudget, MapLimits};
    MapBudget::new(
        MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        stop,
    )
}

#[test]
fn registry_capabilities_are_part_of_the_persistent_cache_namespace() {
    let dir = Fixture::new();
    let policy = workspace_policy(&dir.0);
    let security = ContentSecurity::new();
    let scan = GraphScan {
        fact_cache: Some(dir.0.join(".facts")),
        ..Default::default()
    };
    let installed =
        FactCache::new(&scan, &dir.0, &policy, &security, RegistryView::installed()).unwrap();
    let injected = FactCache::new(
        &scan,
        &dir.0,
        &policy,
        &security,
        octocode_engine::signatures::languages::conformance::toml_registry(),
    )
    .unwrap();
    assert_ne!(installed.namespace, injected.namespace);
    let repeated =
        FactCache::new(&scan, &dir.0, &policy, &security, RegistryView::installed()).unwrap();
    assert_eq!(installed.namespace, repeated.namespace);
}
