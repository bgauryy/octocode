use super::types::Node;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) fn reachable(
    graph: &BTreeMap<String, Node>,
    roots: &[String],
    static_only: bool,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = roots.to_vec();
    seen.extend(roots.iter().cloned());
    while let Some(file) = stack.pop() {
        if let Some(node) = graph.get(&file) {
            for target in node.edges.keys() {
                if static_only && node.dynamic_only.contains(target) {
                    continue;
                }
                if seen.insert(target.clone()) {
                    stack.push(target.clone());
                }
            }
        }
    }
    seen
}

pub(crate) fn reverse(graph: &BTreeMap<String, Node>) -> BTreeMap<String, Node> {
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

pub(crate) fn traverse(graph: &BTreeMap<String, Node>, source: &str, depth: u32) -> Vec<Value> {
    let mut seen = BTreeSet::from([source.to_owned()]);
    let mut queue = VecDeque::from([(source.to_owned(), 0u32)]);
    let mut out = Vec::new();
    while let Some((file, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }
        if let Some(node) = graph.get(&file) {
            for (target, kinds) in &node.edges {
                if seen.insert(target.clone()) {
                    let d = distance + 1;
                    queue.push_back((target.clone(), d));
                    out.push(json!({"file":target,"distance":d,"via":file,"edgeKinds":kinds,"confidence":"syntactic"}));
                }
            }
        }
    }
    out
}

pub(crate) fn shortest_path(graph: &BTreeMap<String, Node>, source: &str, target: &str) -> Value {
    let mut seen = BTreeSet::from([source.to_owned()]);
    let mut previous = BTreeMap::new();
    let mut queue = VecDeque::from([source.to_owned()]);
    while let Some(file) = queue.pop_front() {
        if file == target {
            break;
        }
        if let Some(node) = graph.get(&file) {
            for next in node.edges.keys() {
                if seen.insert(next.clone()) {
                    previous.insert(next.clone(), file.clone());
                    queue.push_back(next.clone());
                }
            }
        }
    }
    if !seen.contains(target) {
        return json!({"found":false,"files":[],"edges":[]});
    }
    let mut files = vec![target.to_owned()];
    while files[0] != source {
        let p = previous[&files[0]].clone();
        files.insert(0, p);
    }
    let edges=files.windows(2).map(|p|json!({"from":p[0],"to":p[1],"edgeKinds":graph[&p[0]].edges[&p[1]],"confidence":"syntactic"})).collect::<Vec<_>>();
    json!({"found":true,"files":files,"edges":edges,"length":files.len(),"complete":true,"confidence":"syntactic"})
}

pub(crate) fn scc(graph: &BTreeMap<String, Node>, real_only: bool) -> Vec<Vec<String>> {
    scc_inner(graph, real_only, true)
}

pub(crate) fn scc_unsorted(graph: &BTreeMap<String, Node>) -> Vec<Vec<String>> {
    scc_inner(graph, true, false)
}

fn scc_inner(
    graph: &BTreeMap<String, Node>,
    real_only: bool,
    sort_members: bool,
) -> Vec<Vec<String>> {
    struct Frame {
        node: String,
        successors: Vec<String>,
        offset: usize,
    }
    let mut index = 0;
    let mut indices = BTreeMap::new();
    let mut low = BTreeMap::new();
    let mut stack = Vec::new();
    let mut on = BTreeSet::new();
    let mut out = Vec::new();
    for root in graph.keys() {
        if indices.contains_key(root) {
            continue;
        }
        indices.insert(root.clone(), index);
        low.insert(root.clone(), index);
        index += 1;
        stack.push(root.clone());
        on.insert(root.clone());
        let mut frames = vec![Frame {
            node: root.clone(),
            successors: graph[root].edges.keys().cloned().collect(),
            offset: 0,
        }];
        while let Some(frame) = frames.last_mut() {
            if let Some(successor) = frame.successors.get(frame.offset).cloned() {
                frame.offset += 1;
                if !indices.contains_key(&successor) {
                    indices.insert(successor.clone(), index);
                    low.insert(successor.clone(), index);
                    index += 1;
                    stack.push(successor.clone());
                    on.insert(successor.clone());
                    frames.push(Frame {
                        node: successor.clone(),
                        successors: graph
                            .get(&successor)
                            .map(|node| node.edges.keys().cloned().collect())
                            .unwrap_or_default(),
                        offset: 0,
                    });
                } else if on.contains(&successor) {
                    let node = frame.node.clone();
                    low.insert(node.clone(), low[&node].min(indices[&successor]));
                }
                continue;
            }
            let completed = frames.pop().expect("frame exists").node;
            if let Some(parent) = frames.last() {
                low.insert(parent.node.clone(), low[&parent.node].min(low[&completed]));
            }
            if low[&completed] == indices[&completed] {
                let mut component = Vec::new();
                loop {
                    let member = stack.pop().expect("Tarjan stack contains component");
                    on.remove(&member);
                    component.push(member.clone());
                    if member == completed {
                        break;
                    }
                }
                if sort_members {
                    component.sort();
                }
                out.push(component);
            }
        }
    }
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

pub(crate) struct Condensed {
    pub components: Vec<Vec<String>>,
    pub component: BTreeMap<String, usize>,
    pub edges: BTreeMap<usize, BTreeSet<usize>>,
    pub layers: Vec<Vec<usize>>,
}
pub(crate) fn condense(graph: &BTreeMap<String, Node>) -> Condensed {
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
            if let Some(&b) = component.get(to)
                && a != b
            {
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
pub(crate) fn transitive_edges(
    edges: &BTreeMap<usize, BTreeSet<usize>>,
) -> BTreeSet<(usize, usize)> {
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

pub(crate) fn cycle_witness(
    graph: &BTreeMap<String, Node>,
    members: &BTreeSet<String>,
) -> Vec<Value> {
    struct Frame {
        node: String,
        successors: Vec<String>,
        offset: usize,
    }

    let successors = |node: &str| {
        graph
            .get(node)
            .map(|value| {
                value
                    .edges
                    .keys()
                    .filter(|target| members.contains(*target))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let mut state = BTreeMap::new();
    let mut parent = BTreeMap::new();
    for root in members {
        if !graph.contains_key(root) || state.contains_key(root) {
            continue;
        }
        state.insert(root.clone(), 1_u8);
        let mut frames = vec![Frame {
            node: root.clone(),
            successors: successors(root),
            offset: 0,
        }];
        while let Some(frame) = frames.last_mut() {
            let successor = frame.successors.get(frame.offset).cloned();
            frame.offset += 1;
            let Some(successor) = successor else {
                state.insert(frame.node.clone(), 2);
                frames.pop();
                continue;
            };
            match state.get(&successor).copied() {
                None => {
                    parent.insert(successor.clone(), frame.node.clone());
                    state.insert(successor.clone(), 1);
                    frames.push(Frame {
                        node: successor.clone(),
                        successors: successors(&successor),
                        offset: 0,
                    });
                }
                Some(1) => {
                    let cycle_end = frame.node.clone();
                    let mut nodes = vec![cycle_end.clone()];
                    while nodes.last().is_some_and(|node| node != &successor) {
                        let Some(last) = nodes.last() else {
                            return Vec::new();
                        };
                        let Some(previous) = parent.get(last) else {
                            return Vec::new();
                        };
                        nodes.push(previous.clone());
                    }
                    nodes.reverse();
                    let mut witness = nodes
                        .windows(2)
                        .map(|pair| (pair[0].clone(), pair[1].clone()))
                        .collect::<Vec<_>>();
                    witness.push((cycle_end, successor));
                    return witness
                        .into_iter()
                        .map(|(from, to)| {
                            let edge_kinds = graph
                                .get(&from)
                                .and_then(|node| node.edges.get(&to))
                                .cloned()
                                .unwrap_or_else(|| BTreeSet::from(["static-import".to_owned()]));
                            json!({"from":from,"to":to,"edgeKinds":edge_kinds})
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
        assert_eq!(rows.last().unwrap()["distance"], 3);
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
}
