//! The health summary: cumulative component dependency of the import graph.
use super::*;

/// Lakos cumulative component dependency, normalized against a balanced
/// binary tree of the same size (NCCD > 1: more coupled than a tree).
pub(super) fn health(v: &View) -> Value {
    // The condensation of the cycle engine over production files: its
    // groups are the `cycle` detector's, so `filesInCycles` counts them.
    let graph = file_graph(v.t, &|f| v.production(f));
    let n = graph.len();
    if n == 0 {
        return json!({});
    }
    let cycles = Cycles::of(&graph);
    let sizes = cycles
        .condensed
        .components
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let k = sizes.len();
    let dag = (0..k)
        .map(|id| &cycles.condensed.edges[&id])
        .collect::<Vec<_>>();
    // Iterators borrow each adjacency set; the DFS and reachability workspace
    // both stay linear in the component count.
    let mut order = Vec::with_capacity(k);
    let mut seen = vec![false; k];
    for start in 0..k {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, dag[start].iter())];
        while let Some((id, next)) = stack.last_mut() {
            if let Some(&target) = next.next() {
                if !seen[target] {
                    seen[target] = true;
                    stack.push((target, dag[target].iter()));
                }
            } else {
                order.push(*id);
                stack.pop();
            }
        }
    }
    // Stream 64 destination components at a time instead of retaining the
    // quadratic transitive-closure matrix. SCC weights preserve exact CCD.
    let mut reach = vec![0u64; k];
    let mut ccd = 0u64;
    for base in (0..k).step_by(64) {
        let end = (base + 64).min(k);
        let weight = sizes[base] as u64;
        let equal_weights = sizes[base..end].iter().all(|&size| size as u64 == weight);
        for &id in &order {
            let mut bits = if (base..end).contains(&id) {
                1u64 << (id - base)
            } else {
                0
            };
            for &next in dag[id] {
                bits |= reach[next];
            }
            reach[id] = bits;
            let reached = if equal_weights {
                u64::from(bits.count_ones()) * weight
            } else {
                let mut reached = 0u64;
                while bits != 0 {
                    reached += sizes[base + bits.trailing_zeros() as usize] as u64;
                    bits &= bits - 1;
                }
                reached
            };
            ccd += reached * sizes[id] as u64;
        }
    }
    let nf = n as f64;
    let balanced = (nf + 1.0) * (nf + 1.0).log2() - nf;
    let tangled = cycles.groups().map(|(_, files)| files.len()).sum::<usize>();
    json!({
        "files": n,
        "ccd": ccd,
        "nccd": if balanced > 0.0 { ((ccd as f64 / balanced) * 1000.0).round() / 1000.0 } else { 0.0 },
        "averageReach": ((ccd as f64 / nf) * 10.0).round() / 10.0,
        "filesInCycles": tangled,
    })
}
