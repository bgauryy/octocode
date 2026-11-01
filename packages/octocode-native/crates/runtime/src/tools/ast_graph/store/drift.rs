//! graphQuery `drift`: the structural difference between two snapshots.
//!
//! Nodes are keyed by their root-relative key, so snapshots of two roots
//! (a checkout of the base branch beside the head) compare. A relation is
//! `(from, edge kind, to)`: a moved line or a changed import style is not a
//! removed and an added relation; an import pair whose load turned from
//! module-load to deferred (or back) is a transition.
use super::format::{GraphTables, NodeKind};
use crate::tools::ast_graph::algorithms::Cycles;
use crate::tools::ast_graph::edges::{Load, load};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Drift {
    /// File rows first (added, removed, changed), then cycles (added,
    /// resolved), transitions, and relations (added, removed).
    pub rows: Vec<Value>,
    pub summary: Value,
}

/// How an import pair loads: at module load, deferred (a dynamic `import()`
/// or a function-local import), or neither (type-only, compile-time).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Timing {
    Static,
    Dynamic,
    Other,
}

/// File keys with their content digests (none for an unparsed file).
fn files(t: &GraphTables) -> BTreeMap<&str, Option<&str>> {
    let digests = t
        .digests
        .iter()
        .map(|(node, digest)| (*node, t.str(*digest)))
        .collect::<BTreeMap<_, _>>();
    t.node_ids()
        .filter(|id| t.nodes[*id as usize].kind == NodeKind::File)
        .map(|id| (t.str(t.nodes[id as usize].key), digests.get(&id).copied()))
        .collect()
}

/// Every cycle group, as its sorted member keys.
fn cycles(t: &GraphTables) -> BTreeSet<Vec<String>> {
    let graph = super::detect::file_graph(t, &|_| true);
    Cycles::of(&graph)
        .groups()
        .map(|(_, files)| {
            let mut files = files.clone();
            files.sort();
            files
        })
        .collect()
}

/// `(from, edge kind, to)` of every edge.
fn relations(t: &GraphTables) -> BTreeSet<(&str, &'static str, &str)> {
    let key = |id: u32| t.str(t.nodes[id as usize].key);
    t.edges
        .iter()
        .map(|edge| (key(edge.src), edge.kind.as_str(), key(edge.dst)))
        .collect()
}

/// The [`Timing`] of each file-to-file import pair: static when any of its
/// imports runs at module load, dynamic when all are deferred.
fn timings(t: &GraphTables) -> BTreeMap<(&str, &str), Timing> {
    let key = |id: u32| t.str(t.nodes[id as usize].key);
    let mut loads = BTreeMap::<(&str, &str), Vec<Load>>::new();
    for edge in &t.edges {
        let file = |id: u32| t.nodes[id as usize].kind == NodeKind::File;
        if edge.kind == super::format::EdgeKind::Imports && file(edge.src) && file(edge.dst) {
            loads
                .entry((key(edge.src), key(edge.dst)))
                .or_default()
                .push(load(t.str(edge.detail)));
        }
    }
    loads
        .into_iter()
        .map(|(pair, loads)| {
            let timing = if loads.contains(&Load::Eager) {
                Timing::Static
            } else if loads.iter().all(|load| *load == Load::Deferred) {
                Timing::Dynamic
            } else {
                Timing::Other
            };
            (pair, timing)
        })
        .collect()
}

pub(super) fn diff(base: &GraphTables, head: &GraphTables) -> Drift {
    let mut rows = Vec::new();
    let (before, after) = (files(base), files(head));
    let added = after.keys().filter(|file| !before.contains_key(*file));
    let removed = before.keys().filter(|file| !after.contains_key(*file));
    let changed = after
        .iter()
        .filter(|(file, digest)| before.get(*file).is_some_and(|old| old != *digest))
        .map(|(file, _)| file);
    let mut counts = BTreeMap::<&str, usize>::new();
    for (change, files) in [
        ("added", added.collect::<Vec<_>>()),
        ("removed", removed.collect()),
        ("changed", changed.collect()),
    ] {
        counts.insert(change, files.len());
        rows.extend(
            files
                .into_iter()
                .map(|file| json!({"category": "file", "change": change, "file": file})),
        );
    }
    let (before, after) = (cycles(base), cycles(head));
    let cycles_added = after.difference(&before).collect::<Vec<_>>();
    let cycles_resolved = before.difference(&after).collect::<Vec<_>>();
    for (change, groups) in [("added", &cycles_added), ("resolved", &cycles_resolved)] {
        rows.extend(
            groups
                .iter()
                .map(|files| json!({"category": "cycle", "change": change, "files": files})),
        );
    }
    let (before, after) = (timings(base), timings(head));
    let mut shifts = [("staticToDynamic", 0usize), ("dynamicToStatic", 0)];
    for (pair, timing) in &after {
        let change = match (before.get(pair), timing) {
            (Some(Timing::Static), Timing::Dynamic) => &mut shifts[0],
            (Some(Timing::Dynamic), Timing::Static) => &mut shifts[1],
            _ => continue,
        };
        change.1 += 1;
        rows.push(
            json!({"category": "transition", "change": change.0, "from": pair.0, "to": pair.1}),
        );
    }
    let (before, after) = (relations(base), relations(head));
    let relations_added = after.difference(&before).collect::<Vec<_>>();
    let relations_removed = before.difference(&after).collect::<Vec<_>>();
    for (change, relations) in [("added", &relations_added), ("removed", &relations_removed)] {
        rows.extend(relations.iter().map(|(from, edge, to)| {
            json!({"category": "relation", "change": change, "from": from, "to": to, "edge": edge})
        }));
    }
    let delta =
        |head: usize, base: usize| crate::num::i64_of_usize(head) - crate::num::i64_of_usize(base);
    let summary = json!({
        "filesAdded": counts["added"],
        "filesRemoved": counts["removed"],
        "filesChanged": counts["changed"],
        "cyclesAdded": cycles_added.len(),
        "cyclesResolved": cycles_resolved.len(),
        "staticToDynamic": shifts[0].1,
        "dynamicToStatic": shifts[1].1,
        "relationsAdded": relations_added.len(),
        "relationsRemoved": relations_removed.len(),
        "nodes": delta(head.nodes.len(), base.nodes.len()),
        "edges": delta(head.edges.len(), base.edges.len()),
    });
    Drift { rows, summary }
}
