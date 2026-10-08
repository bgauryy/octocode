use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct FileGraphNode {
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
    fn build(graph: &'g BTreeMap<String, FileGraphNode>) -> Self {
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

    /// The `from → to` edge; a pair the successor lists lack (never built by
    /// the algorithms here) carries no kinds.
    fn edge(&self, from: u32, to: u32) -> FileEdge {
        FileEdge {
            from: self.name(from).to_owned(),
            to: self.name(to).to_owned(),
            edge_kinds: self.successors[from as usize]
                .iter()
                .find(|successor| successor.id == to)
                .map(|successor| successor.kinds.clone())
                .unwrap_or_default(),
        }
    }
}

pub fn reachable(
    graph: &BTreeMap<String, FileGraphNode>,
    roots: &[String],
    static_only: bool,
) -> BTreeSet<String> {
    let indexed = Indexed::build(graph);
    let mut seen = vec![false; indexed.names.len()];
    let mut out = BTreeSet::new();
    let mut stack = Vec::new();
    for root in roots {
        out.insert(root.clone());
        if let Some(&id) = indexed.ids.get(root.as_str())
            && !seen[id as usize]
        {
            seen[id as usize] = true;
            stack.push(id);
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

pub fn reverse(graph: &BTreeMap<String, FileGraphNode>) -> BTreeMap<String, FileGraphNode> {
    let mut out: BTreeMap<String, FileGraphNode> = graph
        .keys()
        .map(|k| (k.clone(), FileGraphNode::default()))
        .collect();
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

/// One file a breadth-first traversal reached: its distance from the source,
/// the file it was reached from, and the kinds of that edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraversalStep {
    pub file: String,
    pub distance: u32,
    pub via: String,
    pub edge_kinds: BTreeSet<String>,
}

/// One directed file edge of a path or cycle witness, with its kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEdge {
    pub from: String,
    pub to: String,
    pub edge_kinds: BTreeSet<String>,
}

/// A shortest path: its files in order (source first) and their edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePath {
    pub files: Vec<String>,
    pub edges: Vec<FileEdge>,
}

pub fn traverse(
    graph: &BTreeMap<String, FileGraphNode>,
    source: &str,
    depth: u32,
) -> Vec<TraversalStep> {
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
                out.push(TraversalStep {
                    file: indexed.name(successor.id).to_owned(),
                    distance: d,
                    via: indexed.name(id).to_owned(),
                    edge_kinds: successor.kinds.clone(),
                });
            }
        }
    }
    out
}

/// The shortest `source → target` path; `None` when either is not a graph
/// file or the target is unreachable. A file reaches itself in one hop.
pub fn shortest_path(
    graph: &BTreeMap<String, FileGraphNode>,
    source: &str,
    target: &str,
) -> Option<FilePath> {
    if source == target {
        return Some(FilePath {
            files: vec![source.to_owned()],
            edges: Vec::new(),
        });
    }
    let indexed = Indexed::build(graph);
    let source_id = *indexed.ids.get(source)?;
    let target_id = *indexed.ids.get(target)?;
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
        return None;
    }
    let mut path = vec![target_id];
    while path[0] != source_id {
        path.insert(0, previous[path[0] as usize]);
    }
    Some(FilePath {
        files: path.iter().map(|id| indexed.name(*id).to_owned()).collect(),
        edges: path
            .windows(2)
            .map(|pair| indexed.edge(pair[0], pair[1]))
            .collect(),
    })
}

pub fn scc(graph: &BTreeMap<String, FileGraphNode>, real_only: bool) -> Vec<Vec<String>> {
    scc_inner(graph, real_only, true)
}

pub fn scc_unsorted(graph: &BTreeMap<String, FileGraphNode>) -> Vec<Vec<String>> {
    scc_inner(graph, true, false)
}

fn scc_inner(
    graph: &BTreeMap<String, FileGraphNode>,
    real_only: bool,
    sort_members: bool,
) -> Vec<Vec<String>> {
    let indexed = Indexed::build(graph);
    // Graph keys hold ids 0..key_count in sorted order, matching the original
    // root iteration over `graph.keys()`.
    let out =
        strongly_connected_components(0..indexed.key_count as u32, indexed.names.len(), |id| {
            indexed.successors[id as usize]
                .iter()
                .map(|successor| successor.id)
        });
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

pub struct Condensed {
    pub components: Vec<Vec<String>>,
    pub component: BTreeMap<String, usize>,
    pub edges: BTreeMap<usize, BTreeSet<usize>>,
    pub layers: Vec<Vec<usize>>,
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
                        next.push(to);
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
/// Target columns per bitset pass. Each pass keeps `nodes × COLUMN_CHUNK / 8`
/// bytes of descendant bits (32 MiB at 16,384 nodes), so memory stays bounded
/// at any graph size while the work stays O(edges × nodes / 64).
const COLUMN_CHUNK: usize = 16_384;

/// Transitive edges of a DAG (the condensation): `(u, v)` whose target is
/// also reachable through another successor of `u`. One post-order pass per
/// chunk of target columns builds each node's descendant bitset for those
/// columns, so the whole check costs O(edges × nodes / 64) instead of one
/// graph search per edge (review L14: the old search fallback above 16,384
/// components did not finish on large graphs).
pub fn transitive_edges(edges: &BTreeMap<usize, BTreeSet<usize>>) -> BTreeSet<(usize, usize)> {
    transitive_edges_chunked(edges, COLUMN_CHUNK)
}

fn transitive_edges_chunked(
    edges: &BTreeMap<usize, BTreeSet<usize>>,
    chunk: usize,
) -> BTreeSet<(usize, usize)> {
    let size = edges
        .keys()
        .chain(edges.values().flatten())
        .max()
        .map_or(0, |max| max + 1);
    let order = postorder(edges, size);
    let mut out = BTreeSet::new();
    for low in (0..size).step_by(chunk.max(1)) {
        let high = (low + chunk.max(1)).min(size);
        let words = (high - low).div_ceil(64);
        let in_chunk = |node: usize| (low..high).contains(&node).then(|| node - low);
        // Descendants of each node within columns [low, high), itself excluded.
        let mut descendants = vec![0_u64; size * words];
        let mut row = vec![0_u64; words];
        for &node in &order {
            row.fill(0);
            for &successor in edges.get(&node).into_iter().flatten() {
                if let Some(bit) = in_chunk(successor) {
                    row[bit / 64] |= 1 << (bit % 64);
                }
                let below = &descendants[successor * words..(successor + 1) * words];
                for (word, bits) in row.iter_mut().zip(below) {
                    *word |= bits;
                }
            }
            descendants[node * words..(node + 1) * words].copy_from_slice(&row);
        }
        let mut through = vec![0_u64; words];
        for (&source, targets) in edges {
            if !targets.iter().any(|&target| in_chunk(target).is_some()) {
                continue;
            }
            through.fill(0);
            for &successor in targets {
                let below = &descendants[successor * words..(successor + 1) * words];
                for (word, bits) in through.iter_mut().zip(below) {
                    *word |= bits;
                }
            }
            for &target in targets {
                if let Some(bit) = in_chunk(target)
                    && through[bit / 64] >> (bit % 64) & 1 == 1
                {
                    out.insert((source, target));
                }
            }
        }
    }
    out
}

/// Nodes in post-order (every successor before its predecessors).
fn postorder(edges: &BTreeMap<usize, BTreeSet<usize>>, size: usize) -> Vec<usize> {
    let mut visited = vec![false; size];
    let mut order = Vec::with_capacity(size);
    for start in 0..size {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, edges.get(&start).map(|t| t.iter()))];
        while let Some((node, successors)) = stack.last_mut() {
            let next = successors.as_mut().and_then(|iter| {
                iter.by_ref()
                    .copied()
                    .find(|successor| !visited[*successor])
            });
            match next {
                Some(successor) => {
                    visited[successor] = true;
                    stack.push((successor, edges.get(&successor).map(|t| t.iter())));
                }
                None => {
                    order.push(*node);
                    stack.pop();
                }
            }
        }
    }
    order
}

/// Per-edge graph search; the reference the bitset pass is tested against.
#[cfg(test)]
fn transitive_edges_by_search(
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
                    stack.extend(edges.get(&n).into_iter().flat_map(|x| x.iter()).copied());
                }
            }
        }
    }
    out
}

/// Cycle witnesses over one graph: index the graph once, then find a witness
/// cycle for each strongly connected component's members.
pub struct CycleWitnesses<'g>(Indexed<'g>);

impl<'g> CycleWitnesses<'g> {
    pub fn new(graph: &'g BTreeMap<String, FileGraphNode>) -> Self {
        Self(Indexed::build(graph))
    }

    /// One cycle through `members` as its edges; empty when none is found.
    /// An edge the graph lacks (never produced here) defaults to a static import.
    pub fn witness(&self, members: &BTreeSet<String>) -> Vec<FileEdge> {
        struct Frame {
            node: u32,
            offset: usize,
        }
        const UNSET: u32 = u32::MAX;
        let indexed = &self.0;
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
                                let mut edge = indexed.edge(from, to);
                                if edge.edge_kinds.is_empty() {
                                    edge.edge_kinds = BTreeSet::from(["static-import".to_owned()]);
                                }
                                edge
                            })
                            .collect();
                    }
                    _ => {}
                }
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        /// The bitset pass equals the per-edge search on random DAGs (edges
        /// only point from lower to higher ids, so every input is acyclic).
        #[test]
        fn bitset_transitive_edges_match_the_per_edge_search(
            pairs in proptest::collection::vec((0_usize..40, 0_usize..40), 0..160)
        ) {
            let mut edges = BTreeMap::<usize, BTreeSet<usize>>::new();
            for (a, b) in pairs {
                if a < b {
                    edges.entry(a).or_default().insert(b);
                }
            }
            let expected = transitive_edges_by_search(&edges);
            proptest::prop_assert_eq!(&transitive_edges(&edges), &expected);
            // Small chunks exercise the multi-pass column split.
            for chunk in [1, 7, 64] {
                proptest::prop_assert_eq!(&transitive_edges_chunked(&edges, chunk), &expected);
            }
        }
    }

    #[test]
    fn transitive_edges_finds_the_shortcut_of_a_diamond() {
        // 0→1→3, 0→2→3, and the shortcut 0→3.
        let edges = BTreeMap::from([
            (0, BTreeSet::from([1, 2, 3])),
            (1, BTreeSet::from([3])),
            (2, BTreeSet::from([3])),
        ]);
        assert_eq!(transitive_edges(&edges), BTreeSet::from([(0, 3)]));
    }

    /// Review L14: a condensation above the old 16,384-component bitset
    /// limit stays near-linear instead of one graph search per edge. Before
    /// the column-chunked pass this chain-with-shortcuts graph took a full
    /// downstream walk per chain edge.
    #[test]
    fn large_condensation_transitive_edges_are_fast_and_exact() {
        let size = 30_000;
        let mut edges = BTreeMap::<usize, BTreeSet<usize>>::new();
        for node in 0..size - 1 {
            let targets = edges.entry(node).or_default();
            targets.insert(node + 1);
            if node + 2 < size {
                targets.insert(node + 2);
            }
        }
        let started = std::time::Instant::now();
        let transitive = transitive_edges(&edges);
        let elapsed = started.elapsed();
        let expected: BTreeSet<(usize, usize)> = (0..size - 2).map(|n| (n, n + 2)).collect();
        assert_eq!(transitive, expected);
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "transitive reduction took {elapsed:?}"
        );
    }

    fn node(edges: &[&str]) -> FileGraphNode {
        FileGraphNode {
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
        let path = shortest_path(&graph, "entry.ts", "deep.ts").expect("path");
        assert_eq!(path.files, ["entry.ts", "left.ts", "shared.ts", "deep.ts"]);
        assert_eq!(path.edges.len(), 3);
        assert_eq!(path.edges[0].from, "entry.ts");
        assert_eq!(
            path.edges[0].edge_kinds,
            BTreeSet::from(["static-import".to_owned()])
        );
        assert_eq!(shortest_path(&graph, "deep.ts", "entry.ts"), None);
        let rows = traverse(&graph, "entry.ts", 4);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.last()
                .expect("traversal should produce four rows")
                .distance,
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
