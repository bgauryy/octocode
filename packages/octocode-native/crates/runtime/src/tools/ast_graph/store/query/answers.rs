//! Single-node answers: stats, find, node and symbols.
use super::*;

pub(super) fn stats(g: &Graph) -> GraphOutput {
    // The rankings read production code: test edges are left out.
    let imports = EdgeFilter {
        kinds: vec![EdgeKind::Imports],
        weakest: Confidence::Low,
        tests: false,
    };
    let calls = EdgeFilter {
        kinds: vec![EdgeKind::Calls],
        weakest: Confidence::Low,
        tests: false,
    };
    let top = |filter: &EdgeFilter, kind: NodeKind| -> Vec<Value> {
        hubs(g, filter, Some(kind), Direction::In)
            .rows
            .into_iter()
            .take(5)
            .collect()
    };
    let m = &g.manifest;
    GraphOutput::ok(json!({
        "graph": g.summary(),
        "op": "stats",
        "octocodeVersion": m["octocodeVersion"],
        "scan": m["scan"],
        "counts": m["counts"],
        "imports": m["imports"],
        "calls": m["calls"],
        "languages": m["languages"],
        "diagnostics": m["diagnostics"],
        "mostImportedFiles": top(&imports, NodeKind::File),
        "mostImportedPackages": top(&imports, NodeKind::Package),
        "mostCalledSymbols": top(&calls, NodeKind::Symbol),
        "testEdges": "excluded",
        "evidence": m["evidence"],
    }))
}

pub(super) fn find(g: &Graph, text: &str, kind: Option<NodeKind>) -> Page {
    let lower = text.to_lowercase();
    let admit = |id: u32| kind.is_none_or(|k| g.t.nodes[id as usize].kind == k);
    let mut seen = vec![false; g.t.nodes.len()];
    let mut rows = Vec::new();
    let mut emit = |id: u32, how: &str, rows: &mut Vec<Value>| {
        if admit(id) && !seen[id as usize] {
            seen[id as usize] = true;
            let mut row = g.node_json(id);
            row.insert("match".into(), json!(how));
            rows.push(Value::Object(row));
        }
    };
    let exact = g.name_range(&lower, false).collect::<Vec<_>>();
    for id in exact {
        emit(id, "exact", &mut rows);
    }
    let prefix = g.name_range(&lower, true).collect::<Vec<_>>();
    for id in prefix {
        emit(id, "prefix", &mut rows);
    }
    for id in 0..g.t.nodes.len() as u32 {
        if g.t
            .str(g.t.nodes[id as usize].name)
            .to_lowercase()
            .contains(&lower)
        {
            emit(id, "contains", &mut rows);
        }
    }
    for id in 0..g.t.nodes.len() as u32 {
        if g.key(id).to_lowercase().contains(&lower) {
            emit(id, "path", &mut rows);
        }
    }
    Page {
        total: rows.len(),
        rows,
    }
}

pub(super) fn degree_counts<'a>(
    edges: impl Iterator<Item = &'a EdgeRec>,
) -> BTreeMap<&'static str, u64> {
    let mut counts = BTreeMap::new();
    for edge in edges {
        *counts.entry(edge.kind.as_str()).or_default() += 1;
    }
    counts
}

pub(super) fn node(g: &Graph, id: u32, options: &QueryOptions) -> GraphOutput {
    let record = &g.t.nodes[id as usize];
    let mut out = g.node_json(id);
    if record.kind == NodeKind::Symbol && record.parent != NONE {
        out.insert("parent".into(), json!(g.key(record.parent)));
    }
    out.insert("out".into(), json!(degree_counts(g.t.out(id).iter())));
    out.insert("in".into(), json!(degree_counts(g.t.incoming(id))));
    // Each lead is pinned to this snapshot, like every `next`.
    let lead = |op: &str, depth: Option<u32>| {
        let next = QueryOptions {
            op: op.to_owned(),
            target: Some(g.key(id).to_owned()),
            workspace: options.workspace.clone(),
            depth,
            ..QueryOptions::default()
        };
        command(&next, &g.id)
    };
    let hints = match record.kind {
        NodeKind::File => vec![
            lead("symbols", None),
            lead("dependents", None),
            lead("deps", Some(2)),
        ],
        NodeKind::Symbol => vec![lead("callers", None), lead("callees", None)],
        NodeKind::Package => vec![lead("dependents", None)],
    };
    out.insert("next".into(), json!(hints));
    GraphOutput::ok(json!({"graph": g.summary(), "op": "node", "node": out}))
}

/// The symbols `id` directly contains, in edge order.
pub(super) fn contained_symbols(g: &Graph, id: u32) -> Vec<u32> {
    g.t.out(id)
        .iter()
        .filter(|edge| {
            edge.kind == EdgeKind::Contains && g.t.nodes[edge.dst as usize].kind == NodeKind::Symbol
        })
        .map(|edge| edge.dst)
        .collect()
}

/// Every symbol transitively contained in `root`, in source order.
pub(super) fn descendants(g: &Graph, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let children = contained_symbols(g, id);
        for child in children.into_iter().rev() {
            out.push(child);
            stack.push(child);
        }
    }
    out.sort_by_key(|id| (g.t.nodes[*id as usize].line, *id));
    out
}

pub(super) fn symbols(g: &Graph, root: u32) -> (Map<String, Value>, Page) {
    let mut rows = Vec::new();
    let mut stack = vec![(root, 0u32)];
    while let Some((id, depth)) = stack.pop() {
        let mut children = contained_symbols(g, id);
        children.sort_by_key(|child| (g.t.nodes[*child as usize].line, *child));
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
        if id != root {
            let mut row = g.node_json(id);
            row.remove("file");
            row.insert("depth".into(), json!(depth));
            rows.push(Value::Object(row));
        }
    }
    let mut extra = Map::new();
    extra.insert("target".into(), Value::Object(g.node_json(root)));
    (
        extra,
        Page {
            total: rows.len(),
            rows,
        },
    )
}
