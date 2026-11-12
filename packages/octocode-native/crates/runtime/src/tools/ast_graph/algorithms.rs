//! The graph algorithms every surface shares: the one cycle engine (`graph
//! query cycles`, the `cycle` detector and `drift` read [`Cycles`] over a
//! syntactic file graph) and the one PageRank ([`page_rank`]: the `issues`
//! impact percentile and `hubs` `rankBy:"pageRank"`).

use super::edges::{Load, load};
use octocode_engine::graph::{Condensed, FileGraphNode, condense, scc_filtered};
use std::collections::{BTreeMap, BTreeSet};

/// The code graph without its module tree: no data or `rust-module` edges,
/// and no edge from a module file to one of its ancestors in that tree.
fn dependency_graph(nodes: &BTreeMap<String, FileGraphNode>) -> BTreeMap<String, FileGraphNode> {
    let module_tree = |kind: &String| load(kind) == Load::ModuleTree;
    let mut parent = BTreeMap::<&str, &str>::new();
    for (file, node) in nodes {
        for (target, kinds) in &node.edges {
            if kinds.keys().any(module_tree) {
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
                        .filter(|(kind, _)| !matches!(load(kind), Load::Data | Load::ModuleTree))
                        .map(|(kind, line)| (kind.clone(), *line))
                        .collect::<BTreeMap<_, _>>();
                    (!kinds.is_empty()).then(|| (target.clone(), kinds))
                })
                .collect();
            (
                file.clone(),
                FileGraphNode {
                    edges,
                    dynamic_only: node.dynamic_only.clone(),
                },
            )
        })
        .collect()
}

/// What a cycle group's import edges establish about module load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CycleLoad {
    /// Eager edges alone close a loop inside the group.
    Runtime,
    /// A loop closes only with an edge of unknown load: neither proven.
    Unknown,
    /// No loop runs at module load.
    NoLoad,
}

impl CycleLoad {
    /// The `runtime` value of a cycle row.
    pub(super) fn as_json(self) -> serde_json::Value {
        match self {
            Self::Runtime => serde_json::Value::Bool(true),
            Self::Unknown => serde_json::Value::String("unknown".to_owned()),
            Self::NoLoad => serde_json::Value::Bool(false),
        }
    }
}

/// The cycles of a file graph as every cycle surface reports them: the
/// strongly connected groups of the dependency graph (module trees left
/// out), and inside each the groups its runtime edges close.
pub(super) struct Cycles {
    /// The dependency graph: the file graph without its module trees.
    pub uses: BTreeMap<String, FileGraphNode>,
    /// Its condensation; a cycle group is a component of several files or
    /// one importing itself.
    pub condensed: Condensed,
    /// The cycle groups of the runtime subgraph of `uses`.
    runtime_groups: std::sync::OnceLock<Vec<Vec<String>>>,
    /// The cycle groups with edges of unknown load counted as eager.
    possible_groups: std::sync::OnceLock<Vec<Vec<String>>>,
}

impl Cycles {
    pub(super) fn of(nodes: &BTreeMap<String, FileGraphNode>) -> Self {
        let uses = dependency_graph(nodes);
        let condensed = condense(&uses);
        Self {
            uses,
            condensed,
            runtime_groups: std::sync::OnceLock::new(),
            possible_groups: std::sync::OnceLock::new(),
        }
    }

    /// Each cycle group with its component id, in condensation order.
    pub(super) fn groups(&self) -> impl Iterator<Item = (usize, &Vec<String>)> {
        self.condensed
            .components
            .iter()
            .enumerate()
            .filter(|(_, files)| {
                files.len() > 1
                    || self
                        .uses
                        .get(&files[0])
                        .is_some_and(|node| node.edges.contains_key(&files[0]))
            })
    }

    /// What the edges inside `members` (one static group) establish.
    pub(super) fn load_within(&self, members: &BTreeSet<String>) -> CycleLoad {
        let within = |groups: &[Vec<String>]| {
            groups
                .iter()
                .any(|group| group.iter().all(|file| members.contains(file)))
        };
        if within(self.runtime_groups.get_or_init(|| {
            scc_filtered(&self.uses, true, |kinds| {
                kinds.keys().any(|kind| load(kind) == Load::Eager)
            })
        })) {
            return CycleLoad::Runtime;
        }
        if within(self.possible_groups.get_or_init(|| {
            scc_filtered(&self.uses, true, |kinds| {
                kinds
                    .keys()
                    .any(|kind| matches!(load(kind), Load::Eager | Load::Unknown))
            })
        })) {
            return CycleLoad::Unknown;
        }
        CycleLoad::NoLoad
    }
}

/// Damping of [`page_rank`]: the share of rank that follows edges.
const DAMPING: f64 = 0.85;
/// [`page_rank`] stops once an iteration moves the ranks less than this in
/// total (L1).
const CONVERGED: f64 = 1e-6;
/// [`page_rank`] iterations at most.
const ITERATIONS: usize = 50;
/// Relative PageRank resolution: values closer than this fraction of the
/// mean rank are one tie. [`page_rank`] stops at an L1 change below
/// [`CONVERGED`], so finer differences are summation order, not structure.
pub(super) const RANK_TIE: f64 = 1e-9;

/// Weighted PageRank over `members`, node ids into a table of `len` nodes.
/// `out[node]` lists `(target, weight)` edges, targets among `members`: a
/// member passes its damped rank to its targets in proportion to their
/// weights; one without positive out-weight (dangling) spreads it evenly over
/// every member, as the undamped share is. Members' ranks sum to 1; any other
/// node ranks 0. Unit weights give the classic, unweighted PageRank.
pub(super) fn page_rank(len: usize, members: &[u32], out: &[Vec<(u32, f64)>]) -> Vec<f64> {
    let count = members.len().max(1) as f64;
    let mut rank = vec![0.0; len];
    for node in members {
        rank[*node as usize] = 1.0 / count;
    }
    let edges = |node: u32| out.get(node as usize).map_or(&[][..], Vec::as_slice);
    for _ in 0..ITERATIONS {
        let mut next = vec![0.0; len];
        let mut dangling = 0.0;
        for node in members {
            let targets = edges(*node);
            let r = rank[*node as usize];
            let total = targets.iter().map(|(_, weight)| *weight).sum::<f64>();
            if total > 0.0 {
                let share = r / total;
                for (target, weight) in targets {
                    next[*target as usize] += share * weight;
                }
            } else {
                dangling += r;
            }
        }
        let base = (1.0 - DAMPING) / count + DAMPING * dangling / count;
        let mut delta = 0.0;
        for node in members {
            let value = base + DAMPING * next[*node as usize];
            delta += (value - rank[*node as usize]).abs();
            next[*node as usize] = value;
        }
        rank = next;
        if delta < CONVERGED {
            break;
        }
    }
    rank
}

/// The tie class of PageRank `value` among `members` nodes: values equal
/// within [`RANK_TIE`] relative to the mean rank share one class, and a
/// higher class is a higher rank.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a rank is at most 1, so the class is at most members / RANK_TIE, far inside i64"
)]
pub(super) fn rank_tie_class(value: f64, members: usize) -> i64 {
    let mean = 1.0 / members.max(1) as f64;
    (value / mean / RANK_TIE).round() as i64
}

#[cfg(test)]
mod page_rank_tests {
    use super::{page_rank, rank_tie_class};

    fn unit(edges: &[(u32, u32)], len: usize) -> Vec<Vec<(u32, f64)>> {
        let mut out = vec![Vec::new(); len];
        for (src, dst) in edges {
            out[*src as usize].push((*dst, 1.0));
        }
        out
    }

    /// Members share all rank, a non-member none, whatever the shape:
    /// a dangling member gives its rank back to every member.
    #[test]
    fn member_ranks_sum_to_one_and_others_rank_zero() {
        let members = [0, 1, 2, 4];
        let out = unit(&[(0, 1), (1, 2), (0, 2), (4, 4)], 5);
        let rank = page_rank(5, &members, &out);
        let sum = members.iter().map(|m| rank[*m as usize]).sum::<f64>();
        assert!((sum - 1.0).abs() < 1e-9, "{rank:?}");
        assert_eq!(rank[3], 0.0);
        assert!(rank[2] > rank[1] && rank[1] > rank[0], "{rank:?}");
    }

    /// A heavier edge carries more rank: two targets of one source, equal
    /// by degree, rank by their weights.
    #[test]
    fn weights_split_rank_in_proportion() {
        let members = [0, 1, 2];
        let out = vec![vec![(1, 3.0), (2, 1.0)], Vec::new(), Vec::new()];
        let rank = page_rank(3, &members, &out);
        assert!(rank[1] > rank[2], "{rank:?}");
        let unweighted = page_rank(3, &members, &unit(&[(0, 1), (0, 2)], 3));
        assert_eq!(
            rank_tie_class(unweighted[1], 3),
            rank_tie_class(unweighted[2], 3)
        );
    }

    /// Scaling every weight by one factor changes nothing: weights are
    /// relative strengths, not probabilities.
    #[test]
    fn scaling_all_weights_keeps_the_ranks() {
        let members = [0, 1, 2, 3];
        let edges = [(0, 1, 2.0), (1, 2, 1.0), (2, 0, 5.0), (3, 2, 1.0)];
        let mut once = vec![Vec::new(); 4];
        let mut tenfold = vec![Vec::new(); 4];
        for (src, dst, weight) in edges {
            once[src as usize].push((dst, weight));
            tenfold[src as usize].push((dst, weight * 10.0));
        }
        let (a, b) = (
            page_rank(4, &members, &once),
            page_rank(4, &members, &tenfold),
        );
        for member in members {
            let at = member as usize;
            assert_eq!(rank_tie_class(a[at], 4), rank_tie_class(b[at], 4));
        }
    }
}
