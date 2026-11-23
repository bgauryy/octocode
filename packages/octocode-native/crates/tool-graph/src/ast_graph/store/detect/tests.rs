use super::*;
use octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP;

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

/// The shared cases (`test-utils/graph-algorithms/SPEC.md`) through the engine's
/// cycle groups, the ring witness and PageRank, with the cases' nodes and edges
/// in reverse order as well as as written.
#[test]
fn shared_cases_hold_for_the_ring_and_pagerank() {
    use octocode_engine::graph::{FileGraphNode, cycle_group_order, scc};
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../test-utils/graph-algorithms/cases.json"),
    )
    .expect("the shared graph-algorithm cases");
    let cases: Value = serde_json::from_str(&text).expect("cases are JSON");
    let names = |value: &Value| {
        value
            .as_array()
            .expect("a list")
            .iter()
            .map(|item| item.as_str().expect("a string").to_owned())
            .collect::<Vec<_>>()
    };
    for case in cases.as_array().expect("a list of cases") {
        let label = case["name"].as_str().expect("name");
        let expected = &case["expected"];
        for reversed in [false, true] {
            let mut nodes = names(&case["nodes"]);
            let mut edges = case["edges"].as_array().expect("edges").clone();
            if reversed {
                nodes.reverse();
                edges.reverse();
            }
            let mut graph = nodes
                .iter()
                .map(|node| (node.clone(), FileGraphNode::default()))
                .collect::<BTreeMap<_, _>>();
            for edge in &edges {
                let (from, to) = (
                    edge[0].as_str().expect("from"),
                    edge[1].as_str().expect("to"),
                );
                graph.get_mut(from).expect("a node").edges.insert(
                    to.to_owned(),
                    BTreeMap::from([("static-import".to_owned(), 1)]),
                );
            }
            let mut groups = scc(&graph, true);
            groups.sort_by(|a, b| cycle_group_order(a, b));
            let wanted = expected["cycles"]
                .as_array()
                .expect("cycles")
                .iter()
                .map(names)
                .collect::<Vec<_>>();
            assert_eq!(groups, wanted, "{label}: cycle groups");

            // Ids follow the sorted names, as the stored file nodes do.
            let sorted = graph.keys().cloned().collect::<Vec<_>>();
            let id = |name: &str| {
                u32::try_from(sorted.iter().position(|n| n == name).expect("node")).expect("id")
            };
            let adj = |node: u32| {
                graph[&sorted[node as usize]]
                    .edges
                    .keys()
                    .map(|to| id(to))
                    .collect::<Vec<_>>()
            };
            let ring = |start: &str| {
                let group = groups
                    .iter()
                    .find(|group| group.iter().any(|n| n == start))?;
                let members = group.iter().map(|n| id(n)).collect::<BTreeSet<_>>();
                let mut walk = witness_cycle(id(start), &members, &adj);
                walk.pop();
                Some(
                    walk.into_iter()
                        .map(|node| sorted[node as usize].clone())
                        .collect::<Vec<_>>(),
                )
            };
            for (start, wanted) in expected["ring"].as_object().expect("rings") {
                let wanted = (!wanted.is_null()).then(|| names(wanted));
                assert_eq!(ring(start), wanted, "{label}: ring through {start}");
            }
            for group in &groups {
                assert_eq!(
                    ring(&group[0]).expect("a ring")[0],
                    group[0],
                    "{label}: a ring with no queried node starts at the smallest member"
                );
            }

            let ids = (0..sorted.len())
                .map(|node| u32::try_from(node).expect("id"))
                .collect::<Vec<_>>();
            let flow = ids
                .iter()
                .map(|node| {
                    adj(*node)
                        .into_iter()
                        .map(|to| (to, 1.0))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let rank = crate::ast_graph::algorithms::page_rank(ids.len(), &ids, &flow);
            for (name, value) in expected["pagerank"].as_object().expect("ranks") {
                let got = rank[id(name) as usize];
                let wanted = value.as_f64().expect("a rank");
                assert!(
                    (got - wanted).abs() < 1e-4,
                    "{label}: rank of {name}: {got} vs {wanted}"
                );
            }
        }
    }
}

/// A name imported by a component's script block (every extension of
/// [`SCRIPT_BEARING_MARKUP`]) is a reference, as in source code; the same
/// bare word in prose stays a word.
#[test]
fn script_bearing_markup_names_are_references() {
    let dir = tempfile::tempdir().expect("dir");
    let import = "<script>\nimport { run } from './cmd'\n</script>\n";
    let markup = SCRIPT_BEARING_MARKUP
        .iter()
        .map(|ext| format!("component.{ext}"))
        .collect::<Vec<_>>();
    for name in &markup {
        std::fs::write(dir.path().join(name), import).expect("write");
    }
    std::fs::write(dir.path().join("README.md"), "Call run to start.\n").expect("write");
    let hits = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &BTreeSet::from(["run".to_owned()]),
        &|| false,
    )
    .expect("scan");
    let mut expected = markup
        .iter()
        .map(|name| format!("{name}:2"))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(hits.bare.get("run"), Some(&expected));
    assert_eq!(hits.words.get("run"), Some(&vec!["README.md:1".to_owned()]));
}

/// The engine and this crate each name script-bearing markup through
/// [`SCRIPT_BEARING_MARKUP`]: a line that lists all but one of its extensions
/// is a handpicked copy that drifts from it. Only the definition lists them.
#[test]
fn no_source_keeps_its_own_list_of_script_bearing_markup() {
    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for src in octocode_runtime_base::test_support::runtime_crate_sources() {
        sources(&src, &mut files);
    }
    sources(&manifest.join("../engine/src"), &mut files);
    assert!(files.len() > 100, "{} sources found", files.len());
    let literals = SCRIPT_BEARING_MARKUP
        .iter()
        .map(|ext| format!("\"{ext}\""))
        .collect::<Vec<_>>();
    let mut copies = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read source");
        for (index, line) in text.lines().enumerate() {
            let listed = literals.iter().filter(|lit| line.contains(*lit)).count();
            if listed + 1 >= literals.len() && !line.contains("pub const SCRIPT_BEARING_MARKUP") {
                copies.push(format!("{}:{}", file.display(), index + 1));
            }
        }
    }
    assert!(
        copies.is_empty(),
        "derive these from octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP: {copies:?}"
    );
}

/// The scan skips vendored directories, so a vendored copy that mentions every
/// name corroborates nothing; a directory the graph builds from it reads.
#[test]
fn the_mention_scan_skips_vendored_dirs_and_reads_what_the_graph_builds_from() {
    let dir = tempfile::tempdir().expect("dir");
    for name in crate::ast_graph::classify::VENDORED_DIRS {
        std::fs::create_dir_all(dir.path().join(name)).expect("dir");
        std::fs::write(dir.path().join(name).join("copy.ts"), "run();\n").expect("write");
    }
    for name in ["src", "obj"] {
        std::fs::create_dir(dir.path().join(name)).expect("dir");
        std::fs::write(dir.path().join(name).join("app.ts"), "run();\n").expect("write");
    }
    let hits = scan_mentions(
        dir.path(),
        &BTreeSet::new(),
        &BTreeSet::from(["run".to_owned()]),
        &|| false,
    )
    .expect("scan");
    assert_eq!(
        hits.bare.get("run"),
        Some(&vec!["obj/app.ts:1".to_owned(), "src/app.ts:1".to_owned()])
    );
}

#[test]
fn magnitude_is_bounded() {
    assert!((magnitude(0) - 0.6).abs() < 1e-9);
    assert!(magnitude(1_000_000) <= 1.0);
}
