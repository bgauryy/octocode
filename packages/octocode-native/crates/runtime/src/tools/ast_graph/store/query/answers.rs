//! Single-node answers: stats, node and symbols.
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
        "operation": "stats",
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
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<GraphOutput, GraphOutput> {
    let record = g.node_record(id, cancel)?;
    let mut out = g.node_json(id, cancel)?;
    if record.kind == NodeKind::Symbol && record.parent != NONE {
        out.insert("parent".into(), json!(g.key(record.parent)));
    }
    if let Some(reader) = &g.selective
        && reader.borrow().is_point()
    {
        out.insert("out".into(), json!(reader.borrow().degrees(id, false)));
        out.insert("in".into(), json!(reader.borrow().degrees(id, true)));
    } else {
        out.insert("out".into(), json!(degree_counts(g.t.out(id).iter())));
        out.insert("in".into(), json!(degree_counts(g.t.incoming(id))));
    }
    Ok(GraphOutput::ok(
        json!({"graph": g.summary(), "operation": "node", "node": out}),
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

/// The symbols nested under each of `roots`, in root order and source order
/// within each; a symbol under several roots is listed once, under the first.
/// `depth` counts from its root. One root also echoes it as `target`.
pub(super) fn symbols(
    g: &Graph,
    roots: &[u32],
    range: std::ops::Range<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(Map<String, Value>, Page), GraphOutput> {
    let mut window = Window::budgeted(range, g.page_budget);
    let mut listed = std::collections::HashSet::new();
    for &root in roots {
        let mut stack = vec![(root, 0u32)];
        while let Some((id, depth)) = stack.pop() {
            check(cancel)?;
            if id != root && !listed.insert(id) {
                continue;
            }
            let mut children = contained_symbols(g, id);
            children.sort_by_key(|child| (g.t.nodes[*child as usize].line, *child));
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
            if id != root {
                window.try_push(|| {
                    let mut row = g.node_json(id, cancel)?;
                    if !g.compact_provenance {
                        row.remove("file");
                    }
                    row.insert("depth".into(), json!(depth));
                    Ok(Value::Object(row))
                })?;
            }
        }
    }
    let mut extra = Map::new();
    if let [root] = roots {
        extra.insert("target".into(), Value::Object(g.node_json(*root, cancel)?));
    }
    Ok((extra, window.finish()))
}
