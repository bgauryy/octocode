//! Graph primitives: strongly connected components, feedback arcs, witness
//! cycles and articulation points.
use super::*;

// ── Graph primitives ────────────────────────────────────────────────────────

/// Iterative Tarjan over `nodes` using `adj`; components of size ≥ 2.
pub(super) fn strongly_connected(
    nodes: &[u32],
    adj: &dyn Fn(u32) -> Vec<u32>,
    n: usize,
) -> Vec<Vec<u32>> {
    tarjan(nodes, adj, n, &|component| component.len() > 1)
}

/// Tarjan (the engine's one implementation) over `nodes` using `adj`: the
/// components `keep` admits, each sorted by id.
pub(crate) fn tarjan(
    nodes: &[u32],
    adj: &dyn Fn(u32) -> Vec<u32>,
    n: usize,
    keep: &dyn Fn(&[u32]) -> bool,
) -> Vec<Vec<u32>> {
    octocode_engine::graph::strongly_connected_components(nodes.iter().copied(), n, adj)
        .into_iter()
        .filter(|component| keep(component))
        .map(|mut component| {
            component.sort_unstable();
            component
        })
        .collect()
}

/// Eades–Lin–Smyth greedy feedback-arc set inside one SCC: the returned
/// edges, if removed, leave the members acyclic (|FAS| ≤ m/2 − n/6).
/// Degrees are maintained incrementally, so each pick is O(n) and the whole
/// ordering O(n² + m).
pub(super) fn feedback_arcs(members: &[u32], adj: &dyn Fn(u32) -> Vec<u32>) -> Vec<(u32, u32)> {
    let index = members
        .iter()
        .enumerate()
        .map(|(i, m)| (*m, i))
        .collect::<BTreeMap<_, _>>();
    let k = members.len();
    let mut succ = vec![Vec::new(); k];
    let mut pred = vec![Vec::new(); k];
    for (i, m) in members.iter().enumerate() {
        for to in adj(*m) {
            if let Some(&j) = index.get(&to)
                && j != i
            {
                succ[i].push(j);
                pred[j].push(i);
            }
        }
    }
    let mut out_deg = succ
        .iter()
        .map(|s| crate::num::i64_of_usize(s.len()))
        .collect::<Vec<_>>();
    let mut in_deg = pred
        .iter()
        .map(|p| crate::num::i64_of_usize(p.len()))
        .collect::<Vec<_>>();
    let mut alive = vec![true; k];
    let mut left_count = 0usize;
    let mut position = vec![0usize; k];
    let mut right = Vec::new();
    let mut remaining = k;
    let remove =
        |node: usize, alive: &mut Vec<bool>, out_deg: &mut Vec<i64>, in_deg: &mut Vec<i64>| {
            alive[node] = false;
            for &p in &pred[node] {
                if alive[p] {
                    out_deg[p] -= 1;
                }
            }
            for &s in &succ[node] {
                if alive[s] {
                    in_deg[s] -= 1;
                }
            }
        };
    while remaining > 0 {
        let mut progressed = true;
        while progressed {
            progressed = false;
            for node in 0..k {
                if !alive[node] {
                    continue;
                }
                if out_deg[node] == 0 {
                    right.push(node);
                } else if in_deg[node] == 0 {
                    position[node] = left_count;
                    left_count += 1;
                } else {
                    continue;
                }
                remove(node, &mut alive, &mut out_deg, &mut in_deg);
                remaining -= 1;
                progressed = true;
            }
        }
        if let Some(best) = (0..k)
            .filter(|n| alive[*n])
            .max_by_key(|n| (out_deg[*n] - in_deg[*n], std::cmp::Reverse(*n)))
        {
            position[best] = left_count;
            left_count += 1;
            remove(best, &mut alive, &mut out_deg, &mut in_deg);
            remaining -= 1;
        }
    }
    for (offset, node) in right.iter().rev().enumerate() {
        position[*node] = left_count + offset;
    }
    let mut cuts = Vec::new();
    for (from, targets) in succ.iter().enumerate() {
        for to in targets {
            if position[from] > position[*to] {
                cuts.push((members[from], members[*to]));
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// Shortest directed cycle through `start` inside `members`.
pub(super) fn witness_cycle(
    start: u32,
    members: &BTreeSet<u32>,
    adj: &dyn Fn(u32) -> Vec<u32>,
) -> Vec<u32> {
    let mut previous = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut seen = BTreeSet::from([start]);
    while let Some(id) = queue.pop_front() {
        for next in adj(id).into_iter().filter(|x| members.contains(x)) {
            if next == start {
                let mut path = vec![id];
                let mut cursor = id;
                while let Some(prev) = previous.get(&cursor) {
                    path.push(*prev);
                    cursor = *prev;
                }
                path.reverse();
                path.push(start);
                return path;
            }
            if seen.insert(next) {
                previous.insert(next, id);
                queue.push_back(next);
            }
        }
    }
    vec![start]
}

/// Articulation points of the undirected projection with, for each, the
/// size of the smaller side it separates.
pub(super) fn articulation_points(v: &View) -> Vec<(u32, usize)> {
    let n = v.t.nodes.len();
    let neighbors = |id: u32| {
        let mut all = v.out[id as usize].clone();
        all.extend(&v.inc[id as usize]);
        all.sort_unstable();
        all.dedup();
        all
    };
    let mut disc = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut size = vec![1usize; n];
    let mut timer = 0u32;
    let mut result = Vec::new();
    for &root in &v.files {
        if disc[root as usize] != u32::MAX || neighbors(root).is_empty() {
            continue;
        }
        // First pass: component size.
        let mut comp = 0usize;
        {
            let mut seen = BTreeSet::from([root]);
            let mut queue = VecDeque::from([root]);
            while let Some(id) = queue.pop_front() {
                comp += 1;
                for next in neighbors(id) {
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
        let mut separated: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut root_children = 0usize;
        disc[root as usize] = timer;
        low[root as usize] = timer;
        timer += 1;
        let mut frames = vec![(root, u32::MAX, neighbors(root), 0usize)];
        while let Some((id, parent, next, at)) = frames.last_mut() {
            let (id, parent) = (*id, *parent);
            if let Some(&w) = next.get(*at) {
                *at += 1;
                if disc[w as usize] == u32::MAX {
                    disc[w as usize] = timer;
                    low[w as usize] = timer;
                    timer += 1;
                    if id == root {
                        root_children += 1;
                    }
                    frames.push((w, id, neighbors(w), 0));
                } else if w != parent {
                    low[id as usize] = low[id as usize].min(disc[w as usize]);
                }
                continue;
            }
            frames.pop();
            if let Some((p, _, _, _)) = frames.last() {
                let p = *p;
                size[p as usize] += size[id as usize];
                low[p as usize] = low[p as usize].min(low[id as usize]);
                if low[id as usize] >= disc[p as usize] {
                    separated.entry(p).or_default().push(size[id as usize]);
                }
            }
        }
        for (point, pieces) in separated {
            if point == root && root_children < 2 {
                continue;
            }
            let largest = pieces.iter().copied().max().unwrap_or(0);
            let rest = comp.saturating_sub(1 + largest);
            let smaller = if point == root {
                pieces.iter().copied().sum::<usize>() - largest
            } else {
                largest.min(rest)
            };
            result.push((point, smaller));
        }
    }
    result
}
