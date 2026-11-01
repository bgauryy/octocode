use super::*;

fn persisted(source: &str, relative_path: &str) -> GraphFactsTypedEntry {
    let extraction =
        crate::signatures::extract_graph_facts_with_metadata_inner(source, relative_path)
            .expect("facts");
    GraphFactsTypedEntry {
        relative_path: relative_path.to_owned(),
        content_digest: crate::digest::sha256(source.as_bytes()),
        source_stamp: None,
        facts: extraction.facts,
        reference_counts: extraction.reference_counts,
    }
}

fn scan_cached(root: &Path, reuse: &GraphFactsReuse<'_>) -> (GraphFactsTypedEntry, bool) {
    scan_from(root, FactsSource::CachedGraph(reuse))
}

fn scan_from(root: &Path, source: FactsSource<'_>) -> (GraphFactsTypedEntry, bool) {
    let options = GraphFactsScanOptions {
        path: path_string(root),
        ..Default::default()
    };
    let walked = walk_graph_fact_files(&options, &|_, _| true, &|| false).expect("walk");
    scan_walked_graph_facts(
        options,
        walked,
        source,
        &|_| true,
        &|view| Some((view.to_entry(), view.reused)),
        &|| false,
    )
    .expect("scan")
    .kept
    .remove(0)
    .1
}

#[test]
fn persistent_facts_are_reused_only_for_verified_source_and_schema() {
    for invalid in ["none", "digest", "path", "parserSource", "schema"] {
        let fixture = tempfile::tempdir().expect("fixture");
        let path = format!("persisted_{invalid}.rs");
        let source = format!("fn unique_{invalid}() {{}}\n");
        fs::write(fixture.path().join(&path), &source).expect("source");
        let mut entry = persisted(&source, &path);
        match invalid {
            "digest" => entry.content_digest = "old".into(),
            "path" => entry.relative_path = "elsewhere.rs".into(),
            "parserSource" => entry.facts.source_digest = "old".into(),
            "schema" => entry.facts.schema_version = 0,
            _ => {}
        }
        let (actual, reused) = scan_cached(fixture.path(), &|context| {
            assert_eq!(context.relative_path, path);
            assert_eq!(
                context.content_digest,
                crate::digest::sha256(source.as_bytes())
            );
            assert_eq!(context.parser, None);
            Some(entry.clone())
        });
        assert_eq!(reused, invalid == "none", "{invalid}");
        assert_eq!(actual.facts, persisted(&source, &path).facts);
        let updated = format!("fn changed_{invalid}() {{}}\n");
        fs::write(fixture.path().join(&path), &updated).expect("edit");
        let (after, reused) = scan_cached(fixture.path(), &|_| Some(entry.clone()));
        assert!(!reused, "changed bytes cannot reuse {invalid}");
        assert_eq!(after.facts, persisted(&updated, &path).facts);
    }
}

#[test]
fn prior_schema_facts_cannot_be_reused_for_unchanged_bytes() {
    let fixture = tempfile::tempdir().expect("fixture");
    let path = "constructor_cache.java";
    let source = "class C { C(){ super(); } }";
    fs::write(fixture.path().join(path), source).unwrap();
    let expected = persisted(source, path);
    // Fresh facts carry the current schema; any earlier one must be re-extracted.
    assert_eq!(
        expected.facts.schema_version,
        crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
    );
    assert_eq!(expected.facts.calls.len(), 1);
    let mut old = expected.clone();
    old.facts.schema_version = crate::signatures::GRAPH_FACTS_SCHEMA_VERSION - 1;
    old.facts.calls.clear();
    let (actual, reused) = scan_cached(fixture.path(), &|_| Some(old.clone()));
    assert!(!reused);
    assert_eq!(actual.facts, expected.facts);
}

/// A caller that persists facts (`CachedGraph`) owns their reuse: its scan
/// neither fills nor reads the process-wide memo, which only `Graph` scans
/// keep.
#[test]
fn persisted_scans_leave_the_process_memo_to_memo_scans() {
    let fixture = tempfile::tempdir().expect("fixture");
    let unique = fixture
        .path()
        .file_name()
        .expect("name")
        .to_string_lossy()
        .replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    let source = format!("fn memo_{unique}() {{}}\n");
    fs::write(fixture.path().join("memo.rs"), &source).expect("source");
    let (_, reused) = scan_cached(fixture.path(), &|_| None);
    assert!(!reused);
    let (_, reused) = scan_from(fixture.path(), FactsSource::Graph);
    assert!(!reused, "a persisted scan filled the process memo");
    let (_, reused) = scan_cached(fixture.path(), &|_| None);
    assert!(!reused, "a persisted scan read the process memo");
    let (entry, reused) = scan_from(fixture.path(), FactsSource::Graph);
    assert!(reused, "a memo scan reuses its own memo");
    assert_eq!(entry.facts, persisted(&source, "memo.rs").facts);
}
