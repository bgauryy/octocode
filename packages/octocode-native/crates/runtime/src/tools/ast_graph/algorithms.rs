//! File-graph algorithms (engine-owned, typed) and their astTopology JSON.
//! Every algorithm here reads syntactic import edges, so rendered rows carry
//! `confidence: "syntactic"`.

use super::types::Node;
pub(crate) use octocode_engine::graph::{
    Condensed, CycleWitnesses, condense, reachable, reverse, scc, scc_unsorted, transitive_edges,
};
use octocode_engine::graph::{FileEdge, FileGraphNode};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Breadth-first rows from `source` up to `depth` hops.
pub(crate) fn traverse(
    graph: &BTreeMap<String, FileGraphNode>,
    source: &str,
    depth: u32,
) -> Vec<Value> {
    octocode_engine::graph::traverse(graph, source, depth)
        .into_iter()
        .map(|step| {
            json!({"file":step.file,"distance":step.distance,"via":step.via,"edgeKinds":step.edge_kinds,"confidence":"syntactic"})
        })
        .collect()
}

/// The shortest path as `{found, files, edges, length, complete}`.
pub(crate) fn shortest_path(
    graph: &BTreeMap<String, FileGraphNode>,
    source: &str,
    target: &str,
) -> Value {
    match octocode_engine::graph::shortest_path(graph, source, target) {
        Some(path) => {
            let edges = path
                .edges
                .iter()
                .map(|edge| {
                    json!({"from":edge.from,"to":edge.to,"edgeKinds":edge.edge_kinds,"confidence":"syntactic"})
                })
                .collect::<Vec<_>>();
            json!({"found":true,"files":path.files,"edges":edges,"length":path.files.len(),"complete":true,"confidence":"syntactic"})
        }
        None => json!({"found":false,"files":[],"edges":[]}),
    }
}

/// A cycle witness as `[{from, to, edgeKinds}]`.
pub(crate) fn witness_edges(edges: Vec<FileEdge>) -> Vec<Value> {
    edges
        .into_iter()
        .map(|edge| json!({"from":edge.from,"to":edge.to,"edgeKinds":edge.edge_kinds}))
        .collect()
}

/// The file graph without its module tree: no `rust-module` edges, and no
/// edge from a module file to one of its ancestors in that tree.
pub(super) fn dependency_graph(nodes: &BTreeMap<String, Node>) -> BTreeMap<String, Node> {
    let mut parent = BTreeMap::<&str, &str>::new();
    for (file, node) in nodes {
        for (target, kinds) in &node.edges {
            if kinds.contains("rust-module") {
                parent.entry(target.as_str()).or_insert(file.as_str());
            }
        }
    }
    let is_ancestor = |file: &str, target: &str| {
        let mut seen = BTreeSet::new();
        let mut current = file;
        while let Some(&up) = parent.get(current) {
            if up == target {
                return true;
            }
            if !seen.insert(up) {
                return false;
            }
            current = up;
        }
        false
    };
    nodes
        .iter()
        .map(|(file, node)| {
            let edges = node
                .edges
                .iter()
                .filter(|(target, _)| !is_ancestor(file, target))
                .filter_map(|(target, kinds)| {
                    let kinds = kinds
                        .iter()
                        .filter(|kind| *kind != "rust-module")
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    (!kinds.is_empty()).then(|| (target.clone(), kinds))
                })
                .collect();
            (
                file.clone(),
                Node {
                    edges,
                    dynamic_only: node.dynamic_only.clone(),
                },
            )
        })
        .collect()
}

pub(super) fn collect_kinds(g: &BTreeMap<String, Node>, files: &[String]) -> Vec<String> {
    let members = files.iter().collect::<BTreeSet<_>>();
    let mut kinds = BTreeSet::new();
    for f in files {
        if let Some(n) = g.get(f) {
            for (t, k) in &n.edges {
                if members.contains(t) {
                    kinds.extend(k.iter().cloned())
                }
            }
        }
    }
    kinds.into_iter().collect()
}
pub(super) fn runtime_graph(g: &BTreeMap<String, Node>) -> BTreeMap<String, Node> {
    let runtime = BTreeSet::from([
        "static-import",
        "dynamic-import",
        "named-reexport",
        "star-reexport",
        "commonjs-require",
        "create-require",
        "python-import",
    ]);
    g.iter()
        .map(|(f, n)| {
            let edges = n
                .edges
                .iter()
                .filter(|(_, k)| k.iter().any(|x| runtime.contains(x.as_str())))
                .map(|(t, k)| (t.clone(), k.clone()))
                .collect();
            (
                f.clone(),
                Node {
                    edges,
                    dynamic_only: n.dynamic_only.clone(),
                },
            )
        })
        .collect()
}
pub(super) fn layer_map(c: &Condensed) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for (i, l) in c.layers.iter().enumerate() {
        for x in l {
            out.insert(*x, i);
        }
    }
    out
}
pub(super) fn find_transitive(c: &Condensed) -> BTreeSet<(usize, usize)> {
    transitive_edges(&c.edges)
}
pub(super) fn in_degree(g: &BTreeMap<String, Node>) -> BTreeMap<String, u32> {
    let mut d = BTreeMap::new();
    for n in g.values() {
        for t in n.edges.keys() {
            *d.entry(t.clone()).or_default() += 1
        }
    }
    d
}
pub(super) fn dominators(
    g: &BTreeMap<String, Node>,
    source: &str,
) -> BTreeMap<String, Option<String>> {
    struct Frame {
        node: String,
        successors: Vec<String>,
        offset: usize,
    }
    let mut seen = BTreeSet::from([source.to_owned()]);
    let mut postorder = Vec::new();
    let mut frames = vec![Frame {
        node: source.to_owned(),
        successors: g
            .get(source)
            .map(|node| node.edges.keys().cloned().collect())
            .unwrap_or_default(),
        offset: 0,
    }];
    while let Some(frame) = frames.last_mut() {
        if let Some(successor) = frame.successors.get(frame.offset).cloned() {
            frame.offset += 1;
            if seen.insert(successor.clone()) {
                frames.push(Frame {
                    node: successor.clone(),
                    successors: g
                        .get(&successor)
                        .map(|node| node.edges.keys().cloned().collect())
                        .unwrap_or_default(),
                    offset: 0,
                });
            }
            continue;
        }
        // This branch is only reached with a frame on the stack.
        #[allow(clippy::expect_used)]
        let node = frames.pop().expect("frame exists").node;
        postorder.push(node);
    }
    postorder.reverse();
    let order: BTreeMap<String, usize> = postorder
        .iter()
        .enumerate()
        .map(|(index, node)| (node.clone(), index))
        .collect();
    let mut predecessors: BTreeMap<String, BTreeSet<String>> = postorder
        .iter()
        .map(|node| (node.clone(), BTreeSet::new()))
        .collect();
    for node in &postorder {
        for target in g.get(node).into_iter().flat_map(|node| node.edges.keys()) {
            if let Some(entries) = predecessors.get_mut(target) {
                entries.insert(node.clone());
            }
        }
    }
    let mut idom = BTreeMap::from([(source.to_owned(), source.to_owned())]);
    fn intersect(
        mut left: String,
        mut right: String,
        order: &BTreeMap<String, usize>,
        idom: &BTreeMap<String, String>,
    ) -> String {
        while left != right {
            while order[&left] > order[&right] {
                left = idom[&left].clone();
            }
            while order[&right] > order[&left] {
                right = idom[&right].clone();
            }
        }
        left
    }
    loop {
        let mut changed = false;
        for node in postorder.iter().skip(1) {
            let preds = predecessors[node]
                .iter()
                .filter(|pred| idom.contains_key(*pred))
                .cloned()
                .collect::<Vec<_>>();
            let Some(mut next) = preds.first().cloned() else {
                continue;
            };
            for pred in preds.iter().skip(1) {
                next = intersect(pred.clone(), next, &order, &idom);
            }
            if idom.get(node) != Some(&next) {
                idom.insert(node.clone(), next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut out = BTreeMap::from([(source.to_owned(), None)]);
    out.extend(
        postorder
            .into_iter()
            .skip(1)
            .map(|node| (node.clone(), idom.get(&node).cloned())),
    );
    out
}
