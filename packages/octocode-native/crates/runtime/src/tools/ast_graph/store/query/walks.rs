//! Graph walks over the stored tables: bounded walks, shortest paths,
//! cycles and hubs.
use super::*;

/// How a [`walk`] expands.
pub(super) struct Walk<'f> {
    pub(super) direction: Direction,
    pub(super) filter: &'f EdgeFilter,
    pub(super) depth: u32,
    pub(super) kind: Option<NodeKind>,
    /// Inward from a file, also step to the files that use its exports
    /// through a re-exporting module.
    pub(super) reexports: bool,
}

/// Breadth-first expansion from `seeds`; rows are in discovery order.
pub(super) fn walk(g: &Graph, seeds: &[u32], how: &Walk) -> Page {
    let Walk {
        direction,
        filter,
        depth,
        kind,
        reexports,
    } = *how;
    let mut seen = vec![false; g.t.nodes.len()];
    let mut queue = VecDeque::new();
    for seed in seeds {
        seen[*seed as usize] = true;
        queue.push_back((*seed, 0u32));
    }
    let mut rows = Vec::new();
    let mut visited = 0usize;
    while let Some((id, level)) = queue.pop_front() {
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
            if seen[next as usize] {
                continue;
            }
            seen[next as usize] = true;
            visited += 1;
            if visited > MAX_VISITED {
                break;
            }
            queue.push_back((next, level + 1));
            if kind.is_some_and(|k| g.t.nodes[next as usize].kind != k) {
                continue;
            }
            let mut row = g.node_json(next);
            row.insert("depth".into(), json!(level + 1));
            row.insert("from".into(), json!(g.key(id)));
            g.edge_fields(&mut row, edge);
            rows.push(Value::Object(row));
        }
    }
    Page {
        total: rows.len(),
        rows,
    }
}

pub(super) fn path(
    g: &Graph,
    from: u32,
    to: u32,
    direction: Direction,
    filter: &EdgeFilter,
) -> GraphOutput {
    let mut previous = vec![(NONE, None::<&EdgeRec>); g.t.nodes.len()];
    let mut seen = vec![false; g.t.nodes.len()];
    let mut queue = VecDeque::from([from]);
    seen[from as usize] = true;
    while let Some(id) = queue.pop_front() {
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
        return GraphOutput::empty(json!({
            "graph": g.summary(), "op": "path", "found": false, "edges": edges,
            "from": g.key(from), "to": g.key(to),
            "hint": "No directed path over these edges; try --direction both or --edge imports,calls",
        }));
    }
    let mut hops = Vec::new();
    let mut cursor = to;
    while cursor != from {
        let (prev, edge) = previous[cursor as usize];
        let mut row = g.node_json(cursor);
        if let Some(edge) = edge {
            g.edge_fields(&mut row, edge);
        }
        hops.push(Value::Object(row));
        cursor = prev;
    }
    hops.push(Value::Object(g.node_json(from)));
    hops.reverse();
    GraphOutput::ok(json!({
        "graph": g.summary(), "op": "path", "found": true, "edges": edges,
        "length": hops.len() - 1, "path": hops,
    }))
}

/// Iterative Tarjan over the filtered subgraph. Components are returned
/// largest first, then by their smallest key.
pub(super) fn cycle_preview_row(options: &QueryOptions, id: &str, keys: &[String]) -> Value {
    let mut row = json!({
        "size": keys.len(),
        "nodes": keys.iter().take(MAX_CYCLE_MEMBERS).collect::<Vec<_>>(),
    });
    if keys.len() > MAX_CYCLE_MEMBERS {
        let mut detail = options.clone();
        detail.target = keys.first().cloned();
        detail.to = None;
        detail.list = Some("cycleNodes".into());
        row["nodesTruncated"] = json!(true);
        // The preview shows the first members: the listing reads on after them.
        row["nextNodes"] = json!(next_command(&detail, id, MAX_CYCLE_MEMBERS));
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
    let nodes = (0..n as u32).collect::<Vec<_>>();
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
        });
    }
    let rows = components
        .iter()
        .map(|(keys, runtime)| {
            let mut row = cycle_preview_row(options, &g.id, keys);
            if let Some(runtime) = runtime {
                row["runtime"] = json!(runtime);
            }
            row
        })
        .collect::<Vec<_>>();
    Ok(Page {
        total: rows.len(),
        rows,
    })
}

pub(super) fn hubs(
    g: &Graph,
    filter: &EdgeFilter,
    kind: Option<NodeKind>,
    rank_by: Direction,
) -> Page {
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
    let mut ranked = (0..g.t.nodes.len() as u32)
        .filter(|id| kind.is_none_or(|k| g.t.nodes[*id as usize].kind == k))
        .map(|id| {
            let fan_in = distinct(
                g.t.incoming(id)
                    .filter(|e| filter.admits(e))
                    .map(|e| e.src)
                    .collect(),
            );
            let fan_out = distinct(
                g.t.out(id)
                    .iter()
                    .filter(|e| filter.admits(e))
                    .map(|e| e.dst)
                    .collect(),
            );
            (id, fan_in, fan_out)
        })
        .filter(|(_, fan_in, fan_out)| {
            if rank_by == Direction::Out {
                *fan_out > 0
            } else {
                *fan_in > 0
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        let (ka, kb) = if rank_by == Direction::Out {
            (a.2, b.2)
        } else {
            (a.1, b.1)
        };
        kb.cmp(&ka).then_with(|| g.key(a.0).cmp(g.key(b.0)))
    });
    let rows = ranked
        .into_iter()
        .map(|(id, fan_in, fan_out)| {
            let mut row = g.node_json(id);
            row.insert("fanIn".into(), json!(fan_in));
            row.insert("fanOut".into(), json!(fan_out));
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
    }
}
