use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct Node {
    pub edges: BTreeMap<String, BTreeSet<String>>,
    pub dynamic_only: BTreeSet<String>,
}

/// Borrowed index view over the string-keyed file graph. The traversal
/// algorithms below run on `u32` ids and `Vec` state instead of cloning path
/// strings into `BTreeMap` bookkeeping on every visit; strings reappear only
/// at the output boundary. Ids `0..key_count` are the graph keys in sorted
/// order; ids beyond that are edge-only (dangling) targets, and successor
/// lists preserve each node's sorted edge iteration order so outputs match
/// the string-keyed implementation byte for byte.
struct Indexed<'g> {
    names: Vec<&'g str>,
    ids: BTreeMap<&'g str, u32>,
    key_count: usize,
    successors: Vec<Vec<Successor<'g>>>,
}

struct Successor<'g> {
    id: u32,
    kinds: &'g BTreeSet<String>,
    dynamic_only: bool,
}

impl<'g> Indexed<'g> {
    fn build(graph: &'g BTreeMap<String, Node>) -> Self {
        let mut ids = BTreeMap::new();
        let mut names = Vec::with_capacity(graph.len());
        for key in graph.keys() {
            ids.insert(key.as_str(), names.len() as u32);
            names.push(key.as_str());
        }
        let key_count = names.len();
        for node in graph.values() {
            for target in node.edges.keys() {
                if !ids.contains_key(target.as_str()) {
                    ids.insert(target.as_str(), names.len() as u32);
                    names.push(target.as_str());
                }
            }
        }
        let mut successors: Vec<Vec<Successor<'g>>> =
            std::iter::repeat_with(Vec::new).take(names.len()).collect();
        for (from, node) in graph {
            let from_id = ids[from.as_str()] as usize;
            successors[from_id] = node
                .edges
                .iter()
                .map(|(to, kinds)| Successor {
                    id: ids[to.as_str()],
                    kinds,
                    dynamic_only: node.dynamic_only.contains(to),
                })
                .collect();
        }
        Self {
            names,
            ids,
            key_count,
            successors,
        }
    }

    fn name(&self, id: u32) -> &'g str {
        self.names[id as usize]
    }
}

pub fn reachable(
    graph: &BTreeMap<String, Node>,
    roots: &[String],
    static_only: bool,
) -> BTreeSet<String> {
    let indexed = Indexed::build(graph);
    let mut seen = vec![false; indexed.names.len()];
    let mut out = BTreeSet::new();
    let mut stack = Vec::new();
    for root in roots {
        out.insert(root.clone());
        if let Some(&id) = indexed.ids.get(root.as_str()) {
            if !seen[id as usize] {
                seen[id as usize] = true;
                stack.push(id);
            }
        }
    }
    while let Some(id) = stack.pop() {
        for successor in &indexed.successors[id as usize] {
            if static_only && successor.dynamic_only {
                continue;
            }
            if !seen[successor.id as usize] {
                seen[successor.id as usize] = true;
                out.insert(indexed.name(successor.id).to_owned());
                stack.push(successor.id);
            }
        }
    }
    out
}

pub fn reverse(graph: &BTreeMap<String, Node>) -> BTreeMap<String, Node> {
    let mut out: BTreeMap<String, Node> =
        graph.keys().map(|k| (k.clone(), Node::default())).collect();
    for (from, node) in graph {
        for (to, kinds) in &node.edges {
            if let Some(n) = out.get_mut(to) {
                n.edges.insert(from.clone(), kinds.clone());
                if kinds.len() == 1 && kinds.contains("dynamic-import") {
                    n.dynamic_only.insert(from.clone());
                }
            }
        }
    }
    out
}

pub fn traverse(graph: &BTreeMap<String, Node>, source: &str, depth: u32) -> Vec<Value> {
    let indexed = Indexed::build(graph);
    let Some(&source_id) = indexed.ids.get(source) else {
        return Vec::new();
    };
    let mut seen = vec![false; indexed.names.len()];
    seen[source_id as usize] = true;
    let mut queue = VecDeque::from([(source_id, 0u32)]);
    let mut out = Vec::new();
    while let Some((id, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }
        for successor in &indexed.successors[id as usize] {
            if !seen[successor.id as usize] {
                seen[successor.id as usize] = true;
                let d = distance + 1;
                queue.push_back((successor.id, d));
                out.push(json!({"file":indexed.name(successor.id),"distance":d,"via":indexed.name(id),"edgeKinds":successor.kinds,"confidence":"syntactic"}));
            }
        }
    }
    out
}

pub fn shortest_path(graph: &BTreeMap<String, Node>, source: &str, target: &str) -> Value {
    let not_found = || json!({"found":false,"files":[],"edges":[]});
    if source == target {
        return json!({"found":true,"files":[source],"edges":[],"length":1,"complete":true,"confidence":"syntactic"});
    }
    let indexed = Indexed::build(graph);
    let Some(&source_id) = indexed.ids.get(source) else {
        return not_found();
    };
    let Some(&target_id) = indexed.ids.get(target) else {
        return not_found();
    };
    const UNSET: u32 = u32::MAX;
    let mut seen = vec![false; indexed.names.len()];
    let mut previous = vec![UNSET; indexed.names.len()];
    seen[source_id as usize] = true;
    let mut queue = VecDeque::from([source_id]);
    while let Some(id) = queue.pop_front() {
        if id == target_id {
            break;
        }
        for successor in &indexed.successors[id as usize] {
            if !seen[successor.id as usize] {
                seen[successor.id as usize] = true;
                previous[successor.id as usize] = id;
                queue.push_back(successor.id);
            }
        }
    }
    if !seen[target_id as usize] {
        return not_found();
    }
    let mut path = vec![target_id];
    while path[0] != source_id {
        path.insert(0, previous[path[0] as usize]);
    }
    let files = path
        .iter()
        .map(|id| indexed.name(*id).to_owned())
        .collect::<Vec<_>>();
    let edges = path
        .windows(2)
        .map(|pair| {
            let kinds = indexed.successors[pair[0] as usize]
                .iter()
                .find(|successor| successor.id == pair[1])
                .map(|successor| successor.kinds);
            json!({"from":indexed.name(pair[0]),"to":indexed.name(pair[1]),"edgeKinds":kinds,"confidence":"syntactic"})
        })
        .collect::<Vec<_>>();
    json!({"found":true,"files":files,"edges":edges,"length":files.len(),"complete":true,"confidence":"syntactic"})
}

pub fn scc(graph: &BTreeMap<String, Node>, real_only: bool) -> Vec<Vec<String>> {
    scc_inner(graph, real_only, true)
}

pub fn scc_unsorted(graph: &BTreeMap<String, Node>) -> Vec<Vec<String>> {
    scc_inner(graph, true, false)
}

fn scc_inner(
    graph: &BTreeMap<String, Node>,
    real_only: bool,
    sort_members: bool,
) -> Vec<Vec<String>> {
    struct Frame {
        node: u32,
        offset: usize,
    }
    const UNVISITED: u32 = u32::MAX;
    let indexed = Indexed::build(graph);
    let node_count = indexed.names.len();
    let mut index = 0_u32;
    let mut indices = vec![UNVISITED; node_count];
    let mut low = vec![0_u32; node_count];
    let mut stack: Vec<u32> = Vec::new();
    let mut on = vec![false; node_count];
    let mut out: Vec<Vec<u32>> = Vec::new();
    // Graph keys hold ids 0..key_count in sorted order, matching the original
    // root iteration over `graph.keys()`.
    for root in 0..indexed.key_count as u32 {
        if indices[root as usize] != UNVISITED {
            continue;
        }
        indices[root as usize] = index;
        low[root as usize] = index;
        index += 1;
        stack.push(root);
        on[root as usize] = true;
        let mut frames = vec![Frame {
            node: root,
            offset: 0,
        }];
        while let Some(frame) = frames.last_mut() {
            if let Some(successor) = indexed.successors[frame.node as usize]
                .get(frame.offset)
                .map(|successor| successor.id)
            {
                frame.offset += 1;
                if indices[successor as usize] == UNVISITED {
                    indices[successor as usize] = index;
                    low[successor as usize] = index;
                    index += 1;
                    stack.push(successor);
                    on[successor as usize] = true;
                    frames.push(Frame {
                        node: successor,
                        offset: 0,
                    });
                } else if on[successor as usize] {
                    let node = frame.node as usize;
                    low[node] = low[node].min(indices[successor as usize]);
                }
                continue;
            }
            let completed = frames.pop().expect("frame exists").node as usize;
            if let Some(parent) = frames.last() {
                let parent = parent.node as usize;
                low[parent] = low[parent].min(low[completed]);
            }
            if low[completed] == indices[completed] {
                let mut component = Vec::new();
                loop {
                    let member = stack.pop().expect("Tarjan stack contains component");
                    on[member as usize] = false;
                    component.push(member);
                    if member as usize == completed {
                        break;
                    }
                }
                out.push(component);
            }
        }
    }
    let mut out = out
        .into_iter()
        .map(|component| {
            let mut component = component
                .into_iter()
                .map(|id| indexed.name(id).to_owned())
                .collect::<Vec<_>>();
            if sort_members {
                component.sort();
            }
            component
        })
        .collect::<Vec<_>>();
    if sort_members {
        out.sort_by(|a, b| a[0].cmp(&b[0]));
    }
    if real_only {
        out.retain(|c| {
            c.len() > 1
                || graph
                    .get(&c[0])
                    .is_some_and(|n| n.edges.contains_key(&c[0]))
        })
    }
    out
}

pub struct Condensed {
    pub components: Vec<Vec<String>>,
    pub component: BTreeMap<String, usize>,
    pub edges: BTreeMap<usize, BTreeSet<usize>>,
    pub layers: Vec<Vec<usize>>,
}
pub fn condense(graph: &BTreeMap<String, Node>) -> Condensed {
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
    let mut reverse = edges.clone();
    for (from, n) in graph {
        let Some(&a) = component.get(from) else {
            continue;
        };
        for to in n.edges.keys() {
            if let Some(&b) = component.get(to) {
                if a == b {
                    continue;
                }
                if let Some(targets) = edges.get_mut(&a) {
                    targets.insert(b);
                }
                if let Some(sources) = reverse.get_mut(&b) {
                    sources.insert(a);
                }
            }
        }
    }
    let mut indegree: BTreeMap<usize, usize> = reverse.iter().map(|(i, e)| (*i, e.len())).collect();
    let mut frontier = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(i, _)| *i)
        .collect::<Vec<_>>();
    let mut layers = Vec::new();
    while !frontier.is_empty() {
        frontier.sort();
        layers.push(frontier.clone());
        let mut next = Vec::new();
        for from in frontier {
            for to in edges.get(&from).cloned().unwrap_or_default() {
                if let Some(d) = indegree.get_mut(&to) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        next.push(to)
                    }
                }
            }
        }
        frontier = next;
    }
    Condensed {
        components,
        component,
        edges,
        layers,
    }
}
pub fn transitive_edges(edges: &BTreeMap<usize, BTreeSet<usize>>) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for (source, targets) in edges {
        for target in targets {
            let mut seen = BTreeSet::from([*source]);
            let mut stack = targets
                .iter()
                .filter(|x| *x != target)
                .copied()
                .collect::<Vec<_>>();
            while let Some(n) = stack.pop() {
                if n == *target {
                    out.insert((*source, *target));
                    break;
                }
                if seen.insert(n) {
                    stack.extend(edges.get(&n).into_iter().flat_map(|x| x.iter()).copied())
                }
            }
        }
    }
    out
}

pub fn cycle_witness(graph: &BTreeMap<String, Node>, members: &BTreeSet<String>) -> Vec<Value> {
    struct Frame {
        node: u32,
        offset: usize,
    }
    const UNSET: u32 = u32::MAX;
    let indexed = Indexed::build(graph);
    let mut member_mask = vec![false; indexed.names.len()];
    for member in members {
        if let Some(&id) = indexed.ids.get(member.as_str()) {
            member_mask[id as usize] = true;
        }
    }
    // 0 = unvisited, 1 = on the DFS path, 2 = finished.
    let mut state = vec![0_u8; indexed.names.len()];
    let mut parent = vec![UNSET; indexed.names.len()];
    let next_member = |node: u32, offset: &mut usize| {
        while let Some(successor) = indexed.successors[node as usize].get(*offset) {
            *offset += 1;
            if member_mask[successor.id as usize] {
                return Some(successor.id);
            }
        }
        None
    };
    for root_name in members {
        let Some(&root) = indexed.ids.get(root_name.as_str()) else {
            continue;
        };
        if (root as usize) >= indexed.key_count || state[root as usize] != 0 {
            continue;
        }
        state[root as usize] = 1;
        let mut frames = vec![Frame {
            node: root,
            offset: 0,
        }];
        while let Some(frame) = frames.last_mut() {
            let Some(successor) = next_member(frame.node, &mut frame.offset) else {
                state[frame.node as usize] = 2;
                frames.pop();
                continue;
            };
            match state[successor as usize] {
                0 => {
                    parent[successor as usize] = frame.node;
                    state[successor as usize] = 1;
                    frames.push(Frame {
                        node: successor,
                        offset: 0,
                    });
                }
                1 => {
                    let cycle_end = frame.node;
                    let mut nodes = vec![cycle_end];
                    while nodes.last().is_some_and(|node| *node != successor) {
                        let Some(&last) = nodes.last() else {
                            return Vec::new();
                        };
                        let previous = parent[last as usize];
                        if previous == UNSET {
                            return Vec::new();
                        }
                        nodes.push(previous);
                    }
                    nodes.reverse();
                    let mut witness = nodes
                        .windows(2)
                        .map(|pair| (pair[0], pair[1]))
                        .collect::<Vec<_>>();
                    witness.push((cycle_end, successor));
                    return witness
                        .into_iter()
                        .map(|(from, to)| {
                            let edge_kinds = indexed.successors[from as usize]
                                .iter()
                                .find(|successor| successor.id == to)
                                .map(|successor| successor.kinds.clone())
                                .unwrap_or_else(|| BTreeSet::from(["static-import".to_owned()]));
                            json!({"from":indexed.name(from),"to":indexed.name(to),"edgeKinds":edge_kinds})
                        })
                        .collect();
                }
                _ => {}
            }
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(edges: &[&str]) -> Node {
        Node {
            edges: edges
                .iter()
                .map(|target| {
                    (
                        (*target).to_owned(),
                        BTreeSet::from(["static-import".to_owned()]),
                    )
                })
                .collect(),
            dynamic_only: BTreeSet::new(),
        }
    }

    #[test]
    fn shortest_path_is_deterministic_across_a_diamond_and_deep_chain() {
        let graph = BTreeMap::from([
            ("entry.ts".into(), node(&["left.ts", "right.ts"])),
            ("left.ts".into(), node(&["shared.ts"])),
            ("right.ts".into(), node(&["shared.ts"])),
            ("shared.ts".into(), node(&["deep.ts"])),
            ("deep.ts".into(), node(&[])),
        ]);
        assert_eq!(
            shortest_path(&graph, "entry.ts", "deep.ts")["files"],
            json!(["entry.ts", "left.ts", "shared.ts", "deep.ts"])
        );
        let rows = traverse(&graph, "entry.ts", 4);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.last().expect("traversal should produce four rows")["distance"],
            3
        );
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
        fn reversing_twice_preserves_file_topology(
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
                    .entry(to).or_default().insert("static-import".to_owned());
            }
            proptest::prop_assert_eq!(reverse(&reverse(&graph)), graph);
        }

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
                    .entry(to).or_default().insert("static-import".to_owned());
            }
            let components = scc(&graph, false);
            let flattened = components.into_iter().flatten().collect::<BTreeSet<_>>();
            proptest::prop_assert_eq!(flattened, graph.keys().cloned().collect());
        }
    }
}
