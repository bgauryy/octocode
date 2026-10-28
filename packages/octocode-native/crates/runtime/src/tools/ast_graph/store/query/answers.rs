//! Single-node answers: stats, find, node and symbols.
use super::*;

pub(super) fn stats(
    g: &Graph,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<GraphOutput, GraphOutput> {
    // The rankings read production code: test edges are left out.
    let imports = EdgeFilter {
        kinds: vec![EdgeKind::Imports],
        weakest: Confidence::Low,
        tests: false,
    };
    let calls = EdgeFilter {
        kinds: vec![EdgeKind::Calls],
        weakest: Confidence::High,
        tests: false,
    };
    let top = |filter: &EdgeFilter, kind: NodeKind| -> Result<Vec<Value>, GraphOutput> {
        Ok(hubs(g, filter, Some(kind), Direction::In, 0..5, cancel)?.rows)
    };
    let m = &g.manifest;
    Ok(GraphOutput::ok(json!({
        "graph": g.summary(),
        "op": "stats",
        "octocodeVersion": m["octocodeVersion"],
        "scan": m["scan"],
        "counts": m["counts"],
        "imports": m["imports"],
        "calls": m["calls"],
        "argumentCoverage": m["argumentCoverage"],
        "linkage": m["linkage"],
        "linkageByLanguage": m["linkageByLanguage"],
        "extraction": m["extraction"],
        "languages": m["languages"],
        "diagnostics": m["diagnostics"],
        "mostImportedFiles": top(&imports, NodeKind::File)?,
        "mostImportedPackages": top(&imports, NodeKind::Package)?,
        "mostCalledSymbols": top(&calls, NodeKind::Symbol)?,
        "callConfidence": calls.weakest.as_str(),
        "testEdges": "excluded",
        "evidence": m["evidence"],
    })))
}

pub(super) fn find(
    g: &Graph,
    text: &str,
    kind: Option<NodeKind>,
    range: std::ops::Range<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Page, GraphOutput> {
    let lower = text.to_lowercase();
    let mut window = Window::new(range);
    for phase in ["exact", "prefix", "contains", "path"] {
        let ids: Box<dyn Iterator<Item = u32> + '_> = match phase {
            "exact" => Box::new(g.name_range(&lower, false)),
            "prefix" => Box::new(g.name_range(&lower, true)),
            _ => Box::new(0..g.t.nodes.len() as u32),
        };
        for id in ids {
            check(cancel)?;
            if kind.is_some_and(|kind| g.t.nodes[id as usize].kind != kind) {
                continue;
            }
            let name = g.t.str(g.t.nodes[id as usize].name).to_lowercase();
            let admits = match phase {
                "exact" => true,
                "prefix" => name != lower,
                "contains" => name.contains(&lower) && !name.starts_with(&lower),
                _ => !name.contains(&lower) && g.key(id).to_lowercase().contains(&lower),
            };
            if admits {
                window.try_push(|| {
                    let mut row = g.node_json(id, cancel)?;
                    row.insert("match".into(), json!(phase));
                    Ok(Value::Object(row))
                })?;
            }
        }
    }
    Ok(window.finish())
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

pub(super) fn node(
    g: &Graph,
    id: u32,
    options: &QueryOptions,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<GraphOutput, GraphOutput> {
    let record = &g.t.nodes[id as usize];
    let mut out = g.node_json(id, cancel)?;
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
            graph: options.graph.clone(),
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
    Ok(GraphOutput::ok(
        json!({"graph": g.summary(), "op": "node", "node": out}),
    ))
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

pub(super) fn symbols(
    g: &Graph,
    root: u32,
    range: std::ops::Range<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(Map<String, Value>, Page), GraphOutput> {
    let mut window = Window::new(range);
    let mut stack = vec![(root, 0u32)];
    while let Some((id, depth)) = stack.pop() {
        check(cancel)?;
        let mut children = contained_symbols(g, id);
        children.sort_by_key(|child| (g.t.nodes[*child as usize].line, *child));
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
        if id != root {
            window.try_push(|| {
                let mut row = g.node_json(id, cancel)?;
                row.remove("file");
                row.insert("depth".into(), json!(depth));
                Ok(Value::Object(row))
            })?;
        }
    }
    let mut extra = Map::new();
    extra.insert("target".into(), Value::Object(g.node_json(root, cancel)?));
    Ok((extra, window.finish()))
}
