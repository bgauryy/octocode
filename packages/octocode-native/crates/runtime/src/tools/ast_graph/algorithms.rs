//! The one cycle engine of the code graph: every cycle surface (`graph
//! query cycles`, the `cycle` detector, `drift`) reads [`Cycles`] over a
//! syntactic file graph.

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
}

impl Cycles {
    pub(super) fn of(nodes: &BTreeMap<String, FileGraphNode>) -> Self {
        let uses = dependency_graph(nodes);
        let condensed = condense(&uses);
        Self {
            uses,
            condensed,
            runtime_groups: std::sync::OnceLock::new(),
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

    /// The runtime cycle groups inside `members` (one static group).
    pub(super) fn runtime_within(&self, members: &BTreeSet<String>) -> Vec<Vec<String>> {
        self.runtime_groups
            .get_or_init(|| {
                scc_filtered(&self.uses, true, |kinds| {
                    kinds.keys().any(|kind| load(kind) == Load::Eager)
                })
            })
            .iter()
            .filter(|group| group.iter().all(|file| members.contains(file)))
            .cloned()
            .collect()
    }
}
