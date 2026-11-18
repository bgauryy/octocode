use super::*;
use crate::text::analysis::*;

#[test]
fn correspondence_budget_failure_is_terminal_not_an_empty_scan() {
    let fixture = tempfile::tempdir().unwrap();
    fs::write(fixture.path().join("a.rs"), "fn run() {}\n").unwrap();
    let options = GraphFactsScanOptions {
        path: path_string(fixture.path()),
        max_files: Some(10),
        ..Default::default()
    };
    let registry = crate::signatures::languages::RegistryView::installed();
    let walked =
        walk_graph_fact_files_with_registry(&options, &|_, _| true, &|| false, registry).unwrap();
    let result = scan_walked_graph_facts_with_registry(
        options,
        walked,
        FactsSource::Graph,
        &|_| true,
        &|_| Some(()),
        &|| false,
        registry,
        MapLimits {
            bytes: 1,
            ..Default::default()
        },
        None,
    );
    assert!(matches!(
        result,
        Err(GraphScanError::Source(SourceError::LimitExceeded("bytes")))
    ));
}

#[test]
fn raw_identity_distinguishes_canonical_roots_and_is_stable_on_revisit() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for root in [first.path(), second.path()] {
        fs::write(root.join("a.rs"), "fn same() {}\n").unwrap();
    }
    let scan = |root: &Path| {
        let options = GraphFactsScanOptions {
            path: path_string(root),
            max_files: Some(10),
            ..Default::default()
        };
        let walked = walk_graph_fact_files(&options, &|_, _| true, &|| false).unwrap();
        scan_walked_graph_facts(
            options,
            walked,
            FactsSource::Graph,
            &|_| true,
            &|view| {
                Some((
                    view.admitted.snapshot.as_ref().unwrap().id.clone(),
                    view.facts.source_digest.clone(),
                ))
            },
            &|| false,
        )
        .unwrap()
        .kept
        .remove(0)
        .1
    };
    let a = scan(first.path());
    let b = scan(second.path());
    assert_ne!(a.0, b.0);
    assert_eq!(a.1, b.1);
    assert_eq!(a, scan(first.path()));
}

#[cfg(unix)]
#[test]
fn raw_identity_is_the_canonical_path_whatever_spells_the_root_and_links_stay_out() {
    let base = tempfile::tempdir().unwrap();
    let real = base.path().join("real");
    fs::create_dir_all(real.join("nested/deep")).unwrap();
    fs::write(real.join("top.rs"), "fn top() {}\n").unwrap();
    fs::write(real.join("nested/deep/low.rs"), "fn low() {}\n").unwrap();
    // Links inside the root that lead out of it: neither the directory nor
    // the file is walked into or read.
    let outside = base.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("escaped.rs"), "fn escaped() {}\n").unwrap();
    std::os::unix::fs::symlink(&outside, real.join("nested/escape_dir")).unwrap();
    std::os::unix::fs::symlink(outside.join("escaped.rs"), real.join("escape.rs")).unwrap();
    let linked = base.path().join("linked");
    std::os::unix::fs::symlink(&real, &linked).unwrap();
    let scan = |root: &Path| {
        let options = GraphFactsScanOptions {
            path: path_string(root),
            max_files: Some(10),
            ..Default::default()
        };
        let walked = walk_graph_fact_files(&options, &|_, _| true, &|| false).unwrap();
        let mut ids = scan_walked_graph_facts(
            options,
            walked,
            FactsSource::Graph,
            &|_| true,
            &|view| {
                view.admitted
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.id.clone())
            },
            &|| false,
        )
        .unwrap()
        .kept
        .into_iter()
        .collect::<Vec<_>>();
        ids.sort();
        ids
    };
    let canonical = scan(&real.canonicalize().unwrap());
    assert_eq!(canonical.len(), 2);
    assert!(canonical.iter().all(|(path, _)| !path.contains("escape")));
    // The walk refuses a root that is itself a link.
    let refused = GraphFactsScanOptions {
        path: path_string(&linked),
        max_files: Some(10),
        ..Default::default()
    };
    assert!(walk_graph_fact_files(&refused, &|_, _| true, &|| false).is_err());
    assert_eq!(scan(&real.join("nested/../.")), canonical);
}
