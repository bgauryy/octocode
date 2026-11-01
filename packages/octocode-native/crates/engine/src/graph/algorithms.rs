//! The file graph the code graph's cycle engine runs on, and its strongly
//! connected components.
use crate::num::saturating_u32;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The kinds of one file edge, each with the first line of the importing
/// file that evidences it (1-based; 0 when the edge has no source line).
pub type EdgeKinds = BTreeMap<String, u32>;

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct FileGraphNode {
    /// Target file → the kinds of the edge to it.
    pub edges: BTreeMap<String, EdgeKinds>,
    pub dynamic_only: BTreeSet<String>,
}

/// The strongly connected components of `graph`, each sorted, ordered by
/// their first member; `real_only` keeps only cycles (several files, or one
/// importing itself). Tarjan runs on `u32` ids: graph keys hold ids
/// `0..keys` in sorted order, edge-only (dangling) targets the ids after.
pub fn scc(graph: &BTreeMap<String, FileGraphNode>, real_only: bool) -> Vec<Vec<String>> {
    scc_filtered(graph, real_only, |_| true)
}

/// Strongly connected components of a borrowed edge view, without cloning
/// the file graph or its evidence maps.
pub fn scc_filtered(
    graph: &BTreeMap<String, FileGraphNode>,
    real_only: bool,
    keep: impl Fn(&EdgeKinds) -> bool,
) -> Vec<Vec<String>> {
    let mut ids = BTreeMap::new();
    let mut names = Vec::with_capacity(graph.len());
    for key in graph.keys() {
        ids.insert(key.as_str(), saturating_u32(names.len()));
        names.push(key.as_str());
    }
    let keys = names.len();
    for node in graph.values() {
        for (target, _) in node.edges.iter().filter(|(_, kinds)| keep(kinds)) {
            if !ids.contains_key(target.as_str()) {
                ids.insert(target.as_str(), saturating_u32(names.len()));
                names.push(target.as_str());
            }
        }
    }
    let mut successors = vec![Vec::new(); names.len()];
    for (from, node) in graph {
        successors[ids[from.as_str()] as usize] = node
            .edges
            .iter()
            .filter(|(_, kinds)| keep(kinds))
            .map(|(to, _)| ids[to.as_str()])
            .collect::<Vec<_>>();
    }
    let components = strongly_connected_components(0..saturating_u32(keys), names.len(), |id| {
        successors[id as usize].iter().copied()
    });
    let mut out = components
        .into_iter()
        .map(|component| {
            let mut component = component
                .into_iter()
                .map(|id| names[id as usize].to_owned())
                .collect::<Vec<_>>();
            component.sort();
            component
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a[0].cmp(&b[0]));
    if real_only {
        out.retain(|c| {
            c.len() > 1
                || graph
                    .get(&c[0])
                    .and_then(|n| n.edges.get(&c[0]))
                    .is_some_and(&keep)
        });
    }
    out
}

/// Iterative Tarjan over ids `0..node_count`, starting a search from each
/// unvisited id of `roots` in order. Components come in completion order,
/// members in stack-pop order (the root last).
pub fn strongly_connected_components<I, F>(
    roots: impl IntoIterator<Item = u32>,
    node_count: usize,
    mut successors: F,
) -> Vec<Vec<u32>>
where
    F: FnMut(u32) -> I,
    I: IntoIterator<Item = u32>,
{
    const UNVISITED: u32 = u32::MAX;
    let mut index = 0_u32;
    let mut indices = vec![UNVISITED; node_count];
    let mut low = vec![0_u32; node_count];
    let mut stack: Vec<u32> = Vec::new();
    let mut on = vec![false; node_count];
    let mut out: Vec<Vec<u32>> = Vec::new();
    for root in roots {
        if indices[root as usize] != UNVISITED {
            continue;
        }
        indices[root as usize] = index;
        low[root as usize] = index;
        index += 1;
        stack.push(root);
        on[root as usize] = true;
        let mut frames = vec![(root, successors(root).into_iter())];
        while let Some((node, next)) = frames.last_mut() {
            let node = *node as usize;
            if let Some(successor) = next.next() {
                if indices[successor as usize] == UNVISITED {
                    indices[successor as usize] = index;
                    low[successor as usize] = index;
                    index += 1;
                    stack.push(successor);
                    on[successor as usize] = true;
                    frames.push((successor, successors(successor).into_iter()));
                } else if on[successor as usize] {
                    low[node] = low[node].min(indices[successor as usize]);
                }
                continue;
            }
            frames.pop();
            if let Some((parent, _)) = frames.last() {
                let parent = *parent as usize;
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == indices[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on[member as usize] = false;
                    component.push(member);
                    if member as usize == node {
                        break;
                    }
                }
                out.push(component);
            }
        }
    }
    out
}

/// The condensation of a file graph: its strongly connected components
/// (as [`scc`] orders them), each file's component, and the edges between
/// components.
pub struct Condensed {
    pub components: Vec<Vec<String>>,
    pub component: BTreeMap<String, usize>,
    pub edges: BTreeMap<usize, BTreeSet<usize>>,
}

pub fn condense(graph: &BTreeMap<String, FileGraphNode>) -> Condensed {
    let components = scc(graph, false);
    let mut component = BTreeMap::new();
    for (i, c) in components.iter().enumerate() {
        for f in c {
            component.insert(f.clone(), i);
        }
    }
    let mut edges: BTreeMap<usize, BTreeSet<usize>> = (0..components.len())
        .map(|i| (i, BTreeSet::new()))
        .collect();
    for (from, n) in graph {
        let Some(&a) = component.get(from) else {
            continue;
        };
        for to in n.edges.keys() {
            if let Some(&b) = component.get(to)
                && a != b
                && let Some(targets) = edges.get_mut(&a)
            {
                targets.insert(b);
            }
        }
    }
    Condensed {
        components,
        component,
        edges,
    }
}

#[cfg(test)]
mod tests {
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
}
