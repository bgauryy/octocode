use super::*;

fn node(edges: &[&str]) -> FileGraphNode {
    FileGraphNode {
        edges: edges
            .iter()
            .map(|target| {
                (
                    (*target).to_owned(),
                    BTreeMap::from([("static-import".to_owned(), 1)]),
                )
            })
            .collect(),
        dynamic_only: BTreeSet::new(),
    }
}

#[test]
fn filtered_scc_matches_materialized_edges_including_self_and_dangling_targets() {
    let graph = BTreeMap::from([
        ("a".into(), node(&["b", "missing"])),
        ("b".into(), node(&["a"])),
        ("self".into(), node(&["self"])),
        ("isolated".into(), node(&[])),
    ]);
    for rejected in ["a", "b", "self", "missing"] {
        let mut marked = graph.clone();
        for node in marked.values_mut() {
            if let Some(kinds) = node.edges.get_mut(rejected) {
                *kinds = BTreeMap::from([("excluded".into(), 1)]);
            }
        }
        let mut materialized = marked.clone();
        for node in materialized.values_mut() {
            node.edges
                .retain(|_, kinds| !kinds.contains_key("excluded"));
        }
        for real_only in [false, true] {
            assert_eq!(
                scc_filtered(&marked, real_only, |kinds| !kinds.contains_key("excluded")),
                scc(&materialized, real_only)
            );
        }
    }
}

#[test]
fn strongly_connected_components_keep_disconnected_cycles_separate() {
    let graph = BTreeMap::from([
        ("a.ts".into(), node(&["b.ts"])),
        ("b.ts".into(), node(&["a.ts"])),
        ("c.ts".into(), node(&[])),
        ("d.ts".into(), node(&["d.ts"])),
    ]);
    assert_eq!(
        scc(&graph, true),
        vec![
            vec!["a.ts".to_owned(), "b.ts".to_owned()],
            vec!["d.ts".to_owned()]
        ]
    );
}

proptest::proptest! {
    #[test]
    fn strongly_connected_components_partition_every_node_once(
        node_count in 1_u8..12,
        raw_edges in proptest::collection::vec((0_u8..24, 0_u8..24), 0..80)
    ) {
        let mut graph = (0..node_count)
            .map(|index| (format!("{index}.rs"), node(&[])))
            .collect::<BTreeMap<_, _>>();
        for (from, to) in raw_edges {
            let from = format!("{}.rs", from % node_count);
            let to = format!("{}.rs", to % node_count);
            graph.get_mut(&from).expect("node").edges
                .entry(to).or_default().insert("static-import".to_owned(), 1);
        }
        let components = scc(&graph, false);
        let flattened = components.into_iter().flatten().collect::<BTreeSet<_>>();
        proptest::prop_assert_eq!(flattened, graph.keys().cloned().collect());
    }
}
