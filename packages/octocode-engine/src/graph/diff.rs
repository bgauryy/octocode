//! Typed, deterministic structural diff between two code-graph snapshots.
//!
//! This is the "AST-only drift" foundation: it compares two immutable
//! [`CodeGraphSnapshot`]s and reports structural changes without requiring
//! language servers. Key correctness properties enforced here:
//!
//! * **Relation identity is structural** — `(from node id, edge kind, to node
//!   id)`. Evidence range/id is never part of relation identity, so a line
//!   shift surfaces in the evidence delta, not as a dependency removal plus
//!   addition.
//! * **Comparability is explicit** — incompatible facts-schema or root
//!   identity refuses the diff (`comparable = false`, empty deltas). Scan
//!   completeness differences downgrade with a warning so a complete head is
//!   never quietly compared against a truncated baseline as real drift.
//! * **Deterministic and invertible** — outputs are sorted; the added
//!   categories of `diff(a, b)` equal the removed categories of `diff(b, a)`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::algorithms::{scc, Node};
use super::model::{CodeGraphSnapshot, EdgeKind, NodeId};

/// A structural relation identity. Deliberately excludes evidence and ranges.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct RelationKey {
    pub from: NodeId,
    pub kind: EdgeKind,
    pub to: NodeId,
}

/// A relation whose static/dynamic character changed between snapshots.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct StaticDynamicShift {
    pub from: NodeId,
    pub to: NodeId,
    pub base_kind: EdgeKind,
    pub head_kind: EdgeKind,
}

/// A reason two snapshots cannot (or should not) be compared directly.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DiffIncompatibility {
    /// Hard: facts schema differs, so identities are not comparable.
    FactsSchemaChanged { base: u32, head: u32 },
    /// Hard: the scan root differs, so path-based identities are not comparable.
    RootChanged { base: String, head: String },
    /// Warning: one side's scan is incomplete; removed edges may be truncation,
    /// not genuine drift. The diff is still produced but flagged.
    ScanCompletenessDiffers {
        base_complete: bool,
        head_complete: bool,
    },
}

impl DiffIncompatibility {
    /// Hard incompatibilities refuse the diff; warnings only annotate it.
    fn is_hard(&self) -> bool {
        matches!(
            self,
            DiffIncompatibility::FactsSchemaChanged { .. } | DiffIncompatibility::RootChanged { .. }
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeDelta {
    pub added: Vec<NodeId>,
    pub removed: Vec<NodeId>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelationDelta {
    pub added: Vec<RelationKey>,
    pub removed: Vec<RelationKey>,
    pub static_to_dynamic: Vec<StaticDynamicShift>,
    pub dynamic_to_static: Vec<StaticDynamicShift>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDelta {
    pub added: u64,
    pub removed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoolChange {
    pub base: bool,
    pub head: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_complete: Option<BoolChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_complete: Option<BoolChange>,
    pub reasons_added: Vec<String>,
    pub reasons_removed: Vec<String>,
    pub scopes_added: Vec<String>,
    pub scopes_removed: Vec<String>,
}

/// Added and resolved import cycles, each identified by its sorted member set.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CycleDelta {
    pub added: Vec<Vec<String>>,
    pub resolved: Vec<Vec<String>>,
}

/// Signed count deltas (`head - base`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricDelta {
    pub nodes: i64,
    pub edges: i64,
    pub evidence: i64,
    pub observations: i64,
}

/// A typed structural diff between two snapshots.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphDiff {
    pub comparable: bool,
    pub incompatibilities: Vec<DiffIncompatibility>,
    pub files: FileDelta,
    pub nodes: NodeDelta,
    pub relations: RelationDelta,
    pub evidence: EvidenceDelta,
    pub completeness: CompletenessDelta,
    pub cycles: CycleDelta,
    pub metrics: MetricDelta,
}

/// Compute a deterministic structural diff from `base` to `head`.
pub fn diff_graphs(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> GraphDiff {
    let mut incompatibilities = Vec::new();
    if base.snapshot.facts_schema_version != head.snapshot.facts_schema_version {
        incompatibilities.push(DiffIncompatibility::FactsSchemaChanged {
            base: base.snapshot.facts_schema_version,
            head: head.snapshot.facts_schema_version,
        });
    }
    if base.snapshot.root != head.snapshot.root {
        incompatibilities.push(DiffIncompatibility::RootChanged {
            base: base.snapshot.root.clone(),
            head: head.snapshot.root.clone(),
        });
    }
    if base.completeness.scan_complete != head.completeness.scan_complete {
        incompatibilities.push(DiffIncompatibility::ScanCompletenessDiffers {
            base_complete: base.completeness.scan_complete,
            head_complete: head.completeness.scan_complete,
        });
    }
    let comparable = !incompatibilities.iter().any(DiffIncompatibility::is_hard);
    if !comparable {
        return GraphDiff {
            comparable,
            incompatibilities,
            ..Default::default()
        };
    }

    GraphDiff {
        comparable,
        incompatibilities,
        files: file_delta(base, head),
        nodes: node_delta(base, head),
        relations: relation_delta(base, head),
        evidence: evidence_delta(base, head),
        completeness: completeness_delta(base, head),
        cycles: cycle_delta(base, head),
        metrics: metric_delta(base, head),
    }
}

fn file_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> FileDelta {
    let base_files = &base.snapshot.files;
    let head_files = &head.snapshot.files;
    let mut delta = FileDelta::default();
    for (path, digest) in head_files {
        match base_files.get(path) {
            None => delta.added.push(path.clone()),
            Some(base_digest) if base_digest != digest => delta.changed.push(path.clone()),
            Some(_) => {}
        }
    }
    for path in base_files.keys() {
        if !head_files.contains_key(path) {
            delta.removed.push(path.clone());
        }
    }
    delta
}

fn node_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> NodeDelta {
    NodeDelta {
        added: head
            .nodes
            .keys()
            .filter(|id| !base.nodes.contains_key(*id))
            .cloned()
            .collect(),
        removed: base
            .nodes
            .keys()
            .filter(|id| !head.nodes.contains_key(*id))
            .cloned()
            .collect(),
    }
}

fn relation_keys(snapshot: &CodeGraphSnapshot) -> BTreeSet<RelationKey> {
    snapshot
        .edges
        .values()
        .map(|edge| RelationKey {
            from: edge.from.clone(),
            kind: edge.kind.clone(),
            to: edge.to.clone(),
        })
        .collect()
}

fn is_static_import(kind: &EdgeKind) -> bool {
    matches!(kind, EdgeKind::Imports | EdgeKind::Reexports)
}

fn is_dynamic_import(kind: &EdgeKind) -> bool {
    matches!(kind, EdgeKind::DynamicImport)
}

fn relation_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> RelationDelta {
    let base_keys = relation_keys(base);
    let head_keys = relation_keys(head);
    let added: Vec<RelationKey> = head_keys.difference(&base_keys).cloned().collect();
    let removed: Vec<RelationKey> = base_keys.difference(&head_keys).cloned().collect();

    // Classify static <-> dynamic import transitions on the same file pair.
    // These remain in `added`/`removed` (they are structurally different edges)
    // and are additionally surfaced as a transition highlight.
    let index = |keys: &[RelationKey], want: fn(&EdgeKind) -> bool| {
        keys.iter()
            .filter(|key| want(&key.kind))
            .map(|key| ((key.from.clone(), key.to.clone()), key.kind.clone()))
            .collect::<BTreeMap<(NodeId, NodeId), EdgeKind>>()
    };
    let removed_static = index(&removed, is_static_import);
    let added_dynamic = index(&added, is_dynamic_import);
    let removed_dynamic = index(&removed, is_dynamic_import);
    let added_static = index(&added, is_static_import);

    let mut static_to_dynamic = Vec::new();
    for (pair, base_kind) in &removed_static {
        if let Some(head_kind) = added_dynamic.get(pair) {
            static_to_dynamic.push(StaticDynamicShift {
                from: pair.0.clone(),
                to: pair.1.clone(),
                base_kind: base_kind.clone(),
                head_kind: head_kind.clone(),
            });
        }
    }
    let mut dynamic_to_static = Vec::new();
    for (pair, base_kind) in &removed_dynamic {
        if let Some(head_kind) = added_static.get(pair) {
            dynamic_to_static.push(StaticDynamicShift {
                from: pair.0.clone(),
                to: pair.1.clone(),
                base_kind: base_kind.clone(),
                head_kind: head_kind.clone(),
            });
        }
    }
    static_to_dynamic.sort();
    dynamic_to_static.sort();

    RelationDelta {
        added,
        removed,
        static_to_dynamic,
        dynamic_to_static,
    }
}

fn evidence_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> EvidenceDelta {
    let added = head
        .evidence
        .keys()
        .filter(|id| !base.evidence.contains_key(*id))
        .count() as u64;
    let removed = base
        .evidence
        .keys()
        .filter(|id| !head.evidence.contains_key(*id))
        .count() as u64;
    EvidenceDelta { added, removed }
}

fn completeness_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> CompletenessDelta {
    let bc = &base.completeness;
    let hc = &head.completeness;
    let bool_change = |b: bool, h: bool| (b != h).then_some(BoolChange { base: b, head: h });
    let diff = |base_set: &BTreeSet<String>, head_set: &BTreeSet<String>| {
        (
            head_set.difference(base_set).cloned().collect::<Vec<_>>(),
            base_set.difference(head_set).cloned().collect::<Vec<_>>(),
        )
    };
    let (reasons_added, reasons_removed) = diff(&bc.reasons, &hc.reasons);
    let (scopes_added, scopes_removed) =
        diff(&bc.semantic_scopes_complete, &hc.semantic_scopes_complete);
    CompletenessDelta {
        scan_complete: bool_change(bc.scan_complete, hc.scan_complete),
        semantic_complete: bool_change(bc.semantic_complete, hc.semantic_complete),
        reasons_added,
        reasons_removed,
        scopes_added,
        scopes_removed,
    }
}

fn file_of(id: &NodeId) -> Option<String> {
    id.0.strip_prefix("file:").map(str::to_owned)
}

fn file_graph(snapshot: &CodeGraphSnapshot) -> BTreeMap<String, Node> {
    let mut graph: BTreeMap<String, Node> = BTreeMap::new();
    for edge in snapshot.edges.values() {
        let (Some(from), Some(to)) = (file_of(&edge.from), file_of(&edge.to)) else {
            continue;
        };
        graph.entry(to.clone()).or_default();
        let node = graph.entry(from).or_default();
        node.edges.entry(to.clone()).or_default();
        if is_dynamic_import(&edge.kind) {
            node.dynamic_only.insert(to);
        }
    }
    // A target reachable by any static edge is not dynamic-only.
    for edge in snapshot.edges.values() {
        if is_dynamic_import(&edge.kind) {
            continue;
        }
        if let (Some(from), Some(to)) = (file_of(&edge.from), file_of(&edge.to)) {
            if let Some(node) = graph.get_mut(&from) {
                node.dynamic_only.remove(&to);
            }
        }
    }
    graph
}

fn cycles(snapshot: &CodeGraphSnapshot) -> BTreeSet<Vec<String>> {
    scc(&file_graph(snapshot), true).into_iter().collect()
}

fn cycle_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> CycleDelta {
    let base_cycles = cycles(base);
    let head_cycles = cycles(head);
    CycleDelta {
        added: head_cycles.difference(&base_cycles).cloned().collect(),
        resolved: base_cycles.difference(&head_cycles).cloned().collect(),
    }
}

fn metric_delta(base: &CodeGraphSnapshot, head: &CodeGraphSnapshot) -> MetricDelta {
    let delta = |base: usize, head: usize| head as i64 - base as i64;
    MetricDelta {
        nodes: delta(base.nodes.len(), head.nodes.len()),
        edges: delta(base.edges.len(), head.edges.len()),
        evidence: delta(base.evidence.len(), head.evidence.len()),
        observations: delta(base.observations.len(), head.observations.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::CodeGraphBuilder;

    fn snapshot(edges: &[(&str, &str, &str, u32)]) -> CodeGraphSnapshot {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        let mut files: BTreeSet<&str> = BTreeSet::new();
        for (from, to, _, _) in edges {
            files.insert(from);
            files.insert(to);
        }
        for file in &files {
            builder.add_file(file, "x").expect("file");
        }
        for (from, to, relation, line) in edges {
            builder
                .add_file_relation(from, to, relation, *line)
                .expect("relation");
        }
        builder.finish()
    }

    #[test]
    fn added_and_removed_relations_use_structural_identity() {
        let base = snapshot(&[("src/a.rs", "src/b.rs", "rust-use", 1)]);
        let head = snapshot(&[("src/a.rs", "src/c.rs", "rust-use", 1)]);
        let diff = diff_graphs(&base, &head);

        assert!(diff.comparable);
        assert_eq!(diff.relations.added.len(), 1);
        assert_eq!(diff.relations.removed.len(), 1);
        assert_eq!(diff.relations.added[0].to, NodeId::file("src/c.rs"));
        assert_eq!(diff.relations.removed[0].to, NodeId::file("src/b.rs"));
    }

    #[test]
    fn line_shift_is_evidence_drift_not_relation_drift() {
        // Same (from, kind, to); only the evidence range moves.
        let base = snapshot(&[("src/a.rs", "src/b.rs", "rust-use", 1)]);
        let head = snapshot(&[("src/a.rs", "src/b.rs", "rust-use", 9)]);
        let diff = diff_graphs(&base, &head);

        assert!(diff.relations.added.is_empty());
        assert!(diff.relations.removed.is_empty());
        assert_eq!(diff.evidence.added, 1);
        assert_eq!(diff.evidence.removed, 1);
    }

    #[test]
    fn static_to_dynamic_transition_is_classified() {
        let base = snapshot(&[("src/a.rs", "src/b.rs", "rust-use", 1)]);
        let head = snapshot(&[("src/a.rs", "src/b.rs", "dynamic-import", 1)]);
        let diff = diff_graphs(&base, &head);

        assert_eq!(diff.relations.static_to_dynamic.len(), 1);
        let shift = &diff.relations.static_to_dynamic[0];
        assert_eq!(shift.base_kind, EdgeKind::Imports);
        assert_eq!(shift.head_kind, EdgeKind::DynamicImport);
        assert!(diff.relations.dynamic_to_static.is_empty());
    }

    #[test]
    fn diff_is_deterministic_and_inverse_categories_agree() {
        let a = snapshot(&[
            ("src/a.rs", "src/b.rs", "rust-use", 1),
            ("src/b.rs", "src/c.rs", "rust-use", 1),
        ]);
        let b = snapshot(&[
            ("src/a.rs", "src/b.rs", "rust-use", 1),
            ("src/a.rs", "src/d.rs", "rust-use", 1),
        ]);
        let forward = diff_graphs(&a, &b);
        let backward = diff_graphs(&b, &a);

        // Determinism: identical inputs yield identical output.
        assert_eq!(forward, diff_graphs(&a, &b));
        // Inverse agreement: forward.added == backward.removed and vice versa.
        assert_eq!(forward.relations.added, backward.relations.removed);
        assert_eq!(forward.relations.removed, backward.relations.added);
        assert_eq!(forward.files.added, backward.files.removed);
        assert_eq!(forward.metrics.edges, -backward.metrics.edges);
    }

    #[test]
    fn incompatible_facts_schema_refuses_the_diff() {
        let base = CodeGraphBuilder::new("/workspace", 1).finish();
        let head = CodeGraphBuilder::new("/workspace", 2).finish();
        let diff = diff_graphs(&base, &head);

        assert!(!diff.comparable);
        assert!(diff.relations.added.is_empty());
        assert!(matches!(
            diff.incompatibilities.as_slice(),
            [DiffIncompatibility::FactsSchemaChanged { base: 1, head: 2 }]
        ));
    }

    #[test]
    fn scan_completeness_difference_downgrades_but_still_compares() {
        let base = snapshot(&[("src/a.rs", "src/b.rs", "rust-use", 1)]);
        let mut head_builder = CodeGraphBuilder::new("/workspace", 1);
        head_builder.add_file("src/a.rs", "x").expect("file");
        head_builder.add_file("src/b.rs", "x").expect("file");
        head_builder
            .add_file_relation("src/a.rs", "src/b.rs", "rust-use", 1)
            .expect("relation");
        head_builder.mark_incomplete("scan-budget", 3);
        let head = head_builder.finish();

        let diff = diff_graphs(&base, &head);
        assert!(diff.comparable, "completeness difference only downgrades");
        assert!(diff.incompatibilities.iter().any(|item| matches!(
            item,
            DiffIncompatibility::ScanCompletenessDiffers { .. }
        )));
    }

    #[test]
    fn resolved_cycle_is_reported() {
        // a -> b -> a is a cycle in the baseline; head breaks it.
        let base = snapshot(&[
            ("src/a.rs", "src/b.rs", "rust-use", 1),
            ("src/b.rs", "src/a.rs", "rust-use", 1),
        ]);
        let head = snapshot(&[("src/a.rs", "src/b.rs", "rust-use", 1)]);
        let diff = diff_graphs(&base, &head);

        assert_eq!(diff.cycles.resolved.len(), 1);
        assert_eq!(
            diff.cycles.resolved[0],
            vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()]
        );
        assert!(diff.cycles.added.is_empty());
    }
}
