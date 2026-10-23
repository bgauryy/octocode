use super::*;

#[test]
fn feedback_arcs_break_every_cycle() {
    // 0→1→2→0 and 1→0: removing the returned arcs must leave a DAG.
    let adj = |id: u32| match id {
        0 => vec![1],
        1 => vec![2, 0],
        2 => vec![0],
        _ => vec![],
    };
    let cuts = feedback_arcs(&[0, 1, 2], &adj);
    assert!(!cuts.is_empty() && cuts.len() <= 2, "{cuts:?}");
    let pruned = |id: u32| {
        adj(id)
            .into_iter()
            .filter(|to| !cuts.contains(&(id, *to)))
            .collect::<Vec<_>>()
    };
    assert!(strongly_connected(&[0, 1, 2], &pruned, 3).is_empty());
}

#[test]
fn witness_is_the_shortest_cycle_through_the_start() {
    let adj = |id: u32| match id {
        0 => vec![1, 3],
        1 => vec![2],
        2 => vec![0],
        3 => vec![0],
        _ => vec![],
    };
    let members = BTreeSet::from([0, 1, 2, 3]);
    assert_eq!(witness_cycle(0, &members, &adj), vec![0, 3, 0]);
}

/// A name imported by a component's script block (`.vue`, `.svelte`,
/// `.astro`, `.mdx`, `.html`) is a reference, as in source code; the
/// same bare word in prose stays a word.
#[test]
fn script_bearing_markup_names_are_references() {
    let dir = tempfile::tempdir().expect("dir");
    let import = "<script>\nimport { run } from './cmd'\n</script>\n";
    let markup = [
        "App.vue",
        "Page.svelte",
        "Card.astro",
        "doc.mdx",
        "index.html",
    ];
    for name in markup {
        std::fs::write(dir.path().join(name), import).expect("write");
    }
    std::fs::write(dir.path().join("README.md"), "Call run to start.\n").expect("write");
    let (hits, truncated) = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &BTreeSet::from(["run".to_owned()]),
    );
    assert!(!truncated);
    let mut expected = markup.map(|name| format!("{name}:2"));
    expected.sort();
    assert_eq!(hits.references.get("run"), Some(&expected.to_vec()));
    assert_eq!(hits.words.get("run"), Some(&vec!["README.md:1".to_owned()]));
}

#[test]
fn magnitude_is_bounded() {
    assert!((magnitude(0) - 0.6).abs() < 1e-9);
    assert!(magnitude(1_000_000) <= 1.0);
}

/// Abstractness reads kinds the extractors emit; a spelling no
/// extractor produces would never count.
#[test]
fn abstract_kinds_are_emitted_declaration_kinds() {
    for kind in ABSTRACT_KINDS {
        assert!(
            octocode_engine::signatures::DECLARATION_KINDS.contains(kind),
            "{kind}"
        );
    }
}
