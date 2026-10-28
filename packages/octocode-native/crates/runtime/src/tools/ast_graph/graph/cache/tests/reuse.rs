use super::*;

#[test]
fn persisted_cache_retains_small_entries_beyond_128_paths() {
    let dir = Fixture::new();
    let policy = workspace_policy(&dir.0);
    let security = ContentSecurity::new();
    let scan = GraphScan {
        fact_cache: Some(dir.0.join(".facts")),
        ..Default::default()
    };
    let mut facts = GraphFactsDocument::default();
    let stop = || false;
    let mut budget = source_budget(&stop);
    let admitted =
        octocode_engine::text::analysis::AdmittedSource::from_bytes(b"", "fixture.rs", &mut budget)
            .unwrap();
    facts.source_templates = octocode_engine::text::analysis::SourceTemplates::new(
        "",
        octocode_engine::text::analysis::Producer::new("fixture", "1"),
        octocode_engine::text::analysis::AnalysisOperation::GraphFacts,
    )
    .unwrap();
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    let cache =
        FactCache::new(&scan, &dir.0, &policy, &security, RegistryView::installed()).unwrap();
    for i in 0..256 {
        cache.store(&octocode_engine::fact_scan::GraphFactsView {
            admitted: &admitted,
            source: &bound,
            relative_path: &format!("file_{i}.rs"),
            content: "",
            content_digest: "digest",
            source_stamp: None,
            facts: &facts,
            reference_counts: &[],
            reused: false,
        });
    }
    drop(cache);
    let cache =
        FactCache::new(&scan, &dir.0, &policy, &security, RegistryView::installed()).unwrap();
    let hits = (0..256)
        .filter(|i| cache.load(&format!("file_{i}.rs"), "digest").is_some())
        .count();
    assert_eq!(hits, 256, "small entries fit the byte budget");
}

#[test]
fn validated_disk_hit_is_not_rewritten() {
    let dir = Fixture::new();
    let policy = workspace_policy(&dir.0);
    let security = ContentSecurity::new();
    let scan = GraphScan {
        fact_cache: Some(dir.0.join(".facts")),
        ..Default::default()
    };
    let mut facts = GraphFactsDocument::default();
    let stop = || false;
    let mut budget = source_budget(&stop);
    let admitted =
        octocode_engine::text::analysis::AdmittedSource::from_bytes(b"", "fixture.rs", &mut budget)
            .unwrap();
    facts.source_templates = octocode_engine::text::analysis::SourceTemplates::new(
        "",
        octocode_engine::text::analysis::Producer::new("fixture", "1"),
        octocode_engine::text::analysis::AnalysisOperation::GraphFacts,
    )
    .unwrap();
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    let cache =
        FactCache::new(&scan, &dir.0, &policy, &security, RegistryView::installed()).unwrap();
    let mut view = octocode_engine::fact_scan::GraphFactsView {
        admitted: &admitted,
        source: &bound,
        relative_path: "one.rs",
        content: "",
        content_digest: "digest",
        source_stamp: None,
        facts: &facts,
        reference_counts: &[],
        reused: false,
    };
    cache.store(&view);
    assert!(cache.load("one.rs", "digest").is_some());
    let path = cache.disk.as_ref().unwrap().file(&cache.key("one.rs"));
    std::fs::File::open(&path)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    view.reused = true;
    cache.store(&view);
    assert_eq!(
        std::fs::metadata(path).unwrap().modified().unwrap(),
        std::time::SystemTime::UNIX_EPOCH
    );
}

#[test]
fn memory_reuse_is_persisted_and_disk_budget_is_enforced() {
    let dir = Fixture::new();
    let root = dir.0.canonicalize().unwrap();
    let policy = workspace_policy(&root);
    let security = ContentSecurity::new();
    let scan = GraphScan {
        fact_cache: Some(root.join(".facts")),
        ..Default::default()
    };
    let mut facts = GraphFactsDocument::default();
    let stop = || false;
    let mut budget = source_budget(&stop);
    let admitted =
        octocode_engine::text::analysis::AdmittedSource::from_bytes(b"", "fixture.rs", &mut budget)
            .unwrap();
    facts.source_templates = octocode_engine::text::analysis::SourceTemplates::new(
        "",
        octocode_engine::text::analysis::Producer::new("fixture", "1"),
        octocode_engine::text::analysis::AnalysisOperation::GraphFacts,
    )
    .unwrap();
    let bound = facts.source_templates.bind(&admitted, &mut budget).unwrap();
    {
        let mut cache =
            FactCache::new(&scan, &root, &policy, &security, RegistryView::installed()).unwrap();
        cache.config.disk_bytes = 2048;
        for id in 0..32 {
            cache.store(&octocode_engine::fact_scan::GraphFactsView {
                admitted: &admitted,
                source: &bound,
                relative_path: &format!("{id}.rs"),
                content_digest: "digest",
                content: "",
                source_stamp: None,
                facts: &facts,
                reference_counts: &[],
                reused: true,
            });
        }
        assert!(
            cache.load("31.rs", "digest").is_some(),
            "memory reuse still needs persistence"
        );
    }
    let bytes = std::fs::read_dir(root.join(".facts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "entry"))
        .map(|path| std::fs::metadata(path).unwrap().len())
        .sum::<u64>();
    assert!(bytes > 0 && bytes <= 2048, "{bytes}");
}
