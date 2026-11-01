//! Graph walks over the stored tables: bounded walks, shortest paths,
//! cycles and hubs.
use super::*;

/// How a [`walk`] expands.
pub(super) struct Walk<'f> {
    pub(super) direction: Direction,
    pub(super) filter: &'f EdgeFilter,
    pub(super) depth: u32,
    pub(super) kind: Option<NodeKind>,
    /// Call answers include a seed reached by a real edge, without expanding it again.
    pub(super) include_seeds: bool,
    /// Inward from a file, also step to the files that use its exports
    /// through a re-exporting module.
    pub(super) reexports: bool,
    pub(super) window: std::ops::Range<usize>,
}

/// Breadth-first expansion from `seeds`; rows are in discovery order.
pub(super) fn walk(
    g: &Graph,
    seeds: &[u32],
    how: &Walk,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Page, GraphOutput> {
    let Walk {
        direction,
        filter,
        depth,
        kind,
        include_seeds,
        reexports,
        ref window,
    } = *how;
    let mut seen = vec![false; g.t.nodes.len()];
    let mut emitted = vec![false; g.t.nodes.len()];
    let mut queue = VecDeque::new();
    for seed in seeds {
        seen[*seed as usize] = true;
        emitted[*seed as usize] = !include_seeds;
        queue.push_back((*seed, 0u32));
    }
    let mut rows = Vec::new();
    let mut total = 0;
    while let Some((id, level)) = queue.pop_front() {
        check(cancel)?;
        if level >= depth {
            continue;
        }
        let through = (reexports
            && direction != Direction::Out
            && g.t.nodes[id as usize].kind == NodeKind::File)
            .then(|| g.reexport_users(id))
            .into_iter()
            .flatten();
        for (next, edge) in g.neighbors(id, direction, filter).chain(through) {
            if !seen[next as usize] {
                seen[next as usize] = true;
                queue.push_back((next, level + 1));
            }
            if emitted[next as usize] {
                continue;
            }
            emitted[next as usize] = true;
            if kind.is_some_and(|k| g.t.nodes[next as usize].kind != k) {
                continue;
            }
            let position = total;
            total += 1;
            if !window.contains(&position) {
                continue;
            }
            let mut row = g.node_json(next, cancel)?;
            row.insert("depth".into(), json!(level + 1));
            row.insert("from".into(), json!(g.key(id)));
            g.edge_fields(&mut row, edge);
            rows.push(Value::Object(row));
        }
    }
    Ok(Page {
        total,
        rows,
        start: window.start,
    })
}

pub(super) fn path(
    g: &Graph,
    from: u32,
    to: u32,
    direction: Direction,
    filter: &EdgeFilter,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<GraphOutput, GraphOutput> {
    let mut previous = vec![(NONE, None::<&EdgeRec>); g.t.nodes.len()];
    let mut seen = vec![false; g.t.nodes.len()];
    let mut queue = VecDeque::from([from]);
    seen[from as usize] = true;
    while let Some(id) = queue.pop_front() {
        check(cancel)?;
        if id == to {
            break;
        }
        for (next, edge) in g.neighbors(id, direction, filter) {
            if !seen[next as usize] {
                seen[next as usize] = true;
                previous[next as usize] = (id, Some(edge));
                queue.push_back(next);
            }
        }
    }
    let edges = json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    if !seen[to as usize] {
        return Ok(GraphOutput::empty(json!({
            "graph": g.summary(), "operation": "path", "found": false, "edges": edges,
            "from": g.key(from), "to": g.key(to),
            "hint": "No directed path over these edges; try direction both or edges [\"imports\",\"calls\"]",
        })));
    }
    let mut hops = Vec::new();
    let mut cursor = to;
    while cursor != from {
        let (prev, edge) = previous[cursor as usize];
        let mut row = g.node_json(cursor, cancel)?;
        if let Some(edge) = edge {
            g.edge_fields(&mut row, edge);
        }
        hops.push(Value::Object(row));
        cursor = prev;
    }
    hops.push(Value::Object(g.node_json(from, cancel)?));
    hops.reverse();
    Ok(GraphOutput::ok(json!({
        "graph": g.summary(), "operation": "path", "found": true, "edges": edges,
        "length": hops.len() - 1, "path": hops,
    })))
}

/// Iterative Tarjan over the filtered subgraph. Components are returned
/// largest first, then by their smallest key.
pub(super) fn cycle_preview_row(keys: &[String]) -> Value {
    let mut row = json!({
        "size": keys.len(),
        "nodes": keys.iter().take(MAX_CYCLE_MEMBERS).collect::<Vec<_>>(),
    });
    if keys.len() > MAX_CYCLE_MEMBERS {
        // `settle` states the channel that lists the members past the preview.
        row["nodesTruncated"] = json!(true);
    }
    row
}

/// Cycle groups as member keys, each with whether a runtime (module-load)
/// loop runs inside it. Over import edges they are the one cycle engine's
/// groups ([`crate::tools::ast_graph::algorithms::Cycles`], as the `cycle`
/// detector reports them); over other edge kinds (calls),
/// the strongly connected groups of those edges, with no runtime verdict.
pub(super) fn cycle_groups(g: &Graph, filter: &EdgeFilter) -> Vec<(Vec<String>, Option<bool>)> {
    if filter.kinds == [EdgeKind::Imports] {
        let graph = super::super::detect::file_graph(&g.t, &|_| true);
        let cycles = crate::tools::ast_graph::algorithms::Cycles::of(&graph);
        return cycles
            .groups()
            .map(|(_, files)| {
                let members = files
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                (
                    files.clone(),
                    Some(!cycles.runtime_within(&members).is_empty()),
                )
            })
            .collect();
    }
    let n = g.t.nodes.len();
    let successors = |id: u32| {
        g.t.out(id)
            .iter()
            .filter(|edge| filter.admits(edge))
            .map(|edge| edge.dst)
            .collect::<Vec<_>>()
    };
    let nodes = g.t.node_ids().collect::<Vec<_>>();
    super::super::detect::tarjan(&nodes, &successors, n, &|component| {
        component.len() > 1 || successors(component[0]).contains(&component[0])
    })
    .into_iter()
    .map(|component| {
        let keys = component.iter().map(|id| g.key(*id).to_owned()).collect();
        (keys, None)
    })
    .collect()
}

pub(super) fn cycles(
    g: &Graph,
    filter: &EdgeFilter,
    options: &QueryOptions,
) -> Result<Page, GraphOutput> {
    let mut components = cycle_groups(g, filter);
    for (component, _) in &mut components {
        component.sort();
    }
    components.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    if options.list.as_deref() == Some("cycleNodes") {
        let Some(target) = options.target.as_deref() else {
            return Err(GraphOutput::error(
                ErrorCode::InvalidInput,
                "cycleNodes needs a cycle member",
            ));
        };
        let Some((component, _)) = components
            .iter()
            .find(|(component, _)| component.iter().any(|key| key == target))
        else {
            return Err(GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("no cycle contains {target:?}"),
            ));
        };
        let rows = component.iter().map(|key| json!(key)).collect::<Vec<_>>();
        return Ok(Page {
            total: rows.len(),
            rows,
            start: 0,
        });
    }
    let rows = components
        .iter()
        .map(|(keys, runtime)| {
            let mut row = cycle_preview_row(keys);
            if let Some(runtime) = runtime {
                row["runtime"] = json!(runtime);
            }
            row
        })
        .collect::<Vec<_>>();
    Ok(Page {
        total: rows.len(),
        rows,
        start: 0,
    })
}

pub(super) fn hubs(
    g: &Graph,
    filter: &EdgeFilter,
    kind: Option<NodeKind>,
    rank_by: Direction,
    range: std::ops::Range<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Page, GraphOutput> {
    let default_kind = match filter.kinds.as_slice() {
        [EdgeKind::Calls] => Some(NodeKind::Symbol),
        [EdgeKind::Imports] if rank_by == Direction::Out => Some(NodeKind::File),
        _ => None,
    };
    let kind = kind.or(default_kind);
    let distinct = |edges: Vec<u32>| {
        let mut edges = edges;
        edges.sort_unstable();
        edges.dedup();
        edges.len()
    };
    let mut ranked = Vec::new();
    for id in g.t.node_ids() {
        check(cancel)?;
        if kind.is_some_and(|kind| g.t.nodes[id as usize].kind != kind) {
            continue;
        }
        let mut incoming = Vec::new();
        for edge in g.t.incoming(id) {
            check(cancel)?;
            if filter.admits(edge) {
                incoming.push(edge.src);
            }
        }
        let mut outgoing = Vec::new();
        for edge in g.t.out(id) {
            check(cancel)?;
            if filter.admits(edge) {
                outgoing.push(edge.dst);
            }
        }
        let fan_in = distinct(incoming);
        let fan_out = distinct(outgoing);
        if (if rank_by == Direction::Out {
            fan_out
        } else {
            fan_in
        }) > 0
        {
            ranked.push((id, fan_in, fan_out));
        }
    }
    ranked.sort_by(|a, b| {
        let (ka, kb) = if rank_by == Direction::Out {
            (a.2, b.2)
        } else {
            (a.1, b.1)
        };
        kb.cmp(&ka).then_with(|| g.key(a.0).cmp(g.key(b.0)))
    });
    let total = ranked.len();
    let start = range.start;
    let mut rows = Vec::new();
    for (id, fan_in, fan_out) in ranked
        .into_iter()
        .skip(start)
        .take(range.end.saturating_sub(start))
    {
        check(cancel)?;
        let mut row = g.node_json(id, cancel)?;
        row.insert("fanIn".into(), json!(fan_in));
        row.insert("fanOut".into(), json!(fan_out));
        rows.push(Value::Object(row));
    }
    Ok(Page { total, rows, start })
}
