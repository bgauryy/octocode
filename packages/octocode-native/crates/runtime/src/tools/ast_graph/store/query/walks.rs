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
    /// The node the answer states as its `target`: a row reached from it
    /// leaves `from` out, which would restate the target's id.
    pub(super) target: Option<u32>,
    pub(super) lists: WalkRows,
    pub(super) window: std::ops::Range<usize>,
}

/// What a [`walk`] lists.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WalkRows {
    /// Each reached node once, in discovery order.
    Nodes,
    /// Each walked edge as the file of the node it reaches (`file`), for
    /// `view:"files"`.
    Files,
    /// Each walked edge as the directories it joins (`from`, `to`), for
    /// `view:"modules"`.
    Modules,
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
        target,
        lists,
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
            // A seed the answer leaves unlisted is never listed. Grouped
            // views count every edge, also one into a node an earlier edge
            // reached; node rows list each node once.
            if emitted[next as usize] {
                continue;
            }
            if lists == WalkRows::Nodes {
                emitted[next as usize] = true;
            }
            if kind.is_some_and(|k| g.t.nodes[next as usize].kind != k) {
                continue;
            }
            let position = total;
            total += 1;
            if !window.contains(&position) {
                continue;
            }
            match lists {
                WalkRows::Files => {
                    rows.push(json!({"path": g.key(g.file_of(next))}));
                    continue;
                }
                WalkRows::Modules => {
                    rows.push(json!({"from": g.module(edge.src), "to": g.module(edge.dst)}));
                    continue;
                }
                WalkRows::Nodes => {}
            }
            let mut row = g.node_json(next, cancel)?;
            row.insert("depth".into(), json!(level + 1));
            if target != Some(id) {
                row.insert("from".into(), json!(g.key(id)));
            }
            g.edge_fields(&mut row, edge);
            // The source occurrences behind the walked edge.
            row.insert(
                "sites".into(),
                json!(g.sites(cancel)?.behind(edge, filter.tests)),
            );
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

/// Cycle groups as member keys, each with what its edges establish about a
/// runtime (module-load) loop inside it. Over import edges they are the one cycle engine's
/// groups ([`crate::tools::ast_graph::algorithms::Cycles`], as the `cycle`
/// detector reports them); over other edge kinds (calls),
/// the strongly connected groups of those edges, with no runtime verdict.
pub(super) fn cycle_groups(
    g: &Graph,
    filter: &EdgeFilter,
) -> Vec<(
    Vec<String>,
    Option<crate::tools::ast_graph::algorithms::CycleLoad>,
)> {
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
                (files.clone(), Some(cycles.load_within(&members)))
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
                row["runtime"] = runtime.as_json();
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

/// What a `hubs` page orders by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RankBy {
    /// Distinct neighbours in the ranked direction (`fanIn` or `fanOut`).
    Degree,
    /// Weighted PageRank over the admitted edges, each weighted by its sites.
    PageRank,
}

/// One ranked node: its degrees both ways, the sites behind its counted
/// edges, and its PageRank when the page ranks by it.
struct Hub {
    id: u32,
    fan_in: usize,
    fan_out: usize,
    sites: u64,
    rank: Option<f64>,
}

/// The `hubs` ranking over the admitted edges. `direction` picks the counted
/// edges: incoming (`In`) or outgoing (`Out`); PageRank over outgoing edges
/// runs on the reversed graph, so a node that depends on many important
/// nodes ranks high.
pub(super) fn hubs(
    g: &Graph,
    filter: &EdgeFilter,
    kind: Option<NodeKind>,
    direction: Direction,
    rank_by: RankBy,
    range: std::ops::Range<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Page, GraphOutput> {
    let default_kind = match filter.kinds.as_slice() {
        [EdgeKind::Calls] => Some(NodeKind::Symbol),
        [EdgeKind::Imports] if direction == Direction::Out => Some(NodeKind::File),
        _ => None,
    };
    let kind = kind.or(default_kind);
    let sites = g.sites(cancel)?;
    let n = g.t.nodes.len();
    let distinct = |mut neighbours: Vec<u32>| {
        neighbours.sort_unstable();
        neighbours.dedup();
        neighbours.len()
    };
    // PageRank flows along each counted edge: importer → imported for
    // incoming, reversed for outgoing.
    let mut flow = vec![Vec::new(); if rank_by == RankBy::PageRank { n } else { 0 }];
    let mut member = vec![false; n];
    let mut hubs = Vec::new();
    for id in g.t.node_ids() {
        check(cancel)?;
        let mut incoming = Vec::new();
        let mut counted_sites = 0;
        for edge in g.t.incoming(id) {
            check(cancel)?;
            if filter.admits(edge) {
                incoming.push(edge.src);
                if direction != Direction::Out {
                    counted_sites += sites.behind(edge, filter.tests);
                }
            }
        }
        let mut outgoing = Vec::new();
        for edge in g.t.out(id) {
            check(cancel)?;
            if filter.admits(edge) {
                outgoing.push(edge.dst);
                let weight = sites.behind(edge, filter.tests);
                if direction == Direction::Out {
                    counted_sites += weight;
                }
                if rank_by == RankBy::PageRank {
                    let (from, to) = if direction == Direction::Out {
                        (edge.dst, edge.src)
                    } else {
                        (edge.src, edge.dst)
                    };
                    flow[from as usize].push((to, weight as f64));
                    member[edge.src as usize] = true;
                    member[edge.dst as usize] = true;
                }
            }
        }
        let (fan_in, fan_out) = (distinct(incoming), distinct(outgoing));
        let counted = if direction == Direction::Out {
            fan_out
        } else {
            fan_in
        };
        // A degree ranking lists the nodes with a counted edge; a PageRank
        // lists every node the admitted edges touch (all its rank).
        let listed = match rank_by {
            RankBy::Degree => counted > 0,
            RankBy::PageRank => fan_in + fan_out > 0,
        };
        if listed && kind.is_none_or(|kind| g.t.nodes[id as usize].kind == kind) {
            hubs.push(Hub {
                id,
                fan_in,
                fan_out,
                sites: counted_sites,
                rank: None,
            });
        }
    }
    match rank_by {
        RankBy::Degree => {
            let counted = |hub: &Hub| {
                if direction == Direction::Out {
                    hub.fan_out
                } else {
                    hub.fan_in
                }
            };
            hubs.sort_by(|a, b| {
                counted(b)
                    .cmp(&counted(a))
                    .then_with(|| g.key(a.id).cmp(g.key(b.id)))
            });
        }
        RankBy::PageRank => {
            let members =
                g.t.node_ids()
                    .filter(|id| member[*id as usize])
                    .collect::<Vec<_>>();
            let rank = crate::tools::ast_graph::algorithms::page_rank(n, &members, &flow);
            for hub in &mut hubs {
                hub.rank = Some(rank[hub.id as usize]);
            }
            // Ranks equal within the PageRank resolution tie; the key
            // orders a tie.
            let class = |hub: &Hub| {
                crate::tools::ast_graph::algorithms::rank_tie_class(
                    rank[hub.id as usize],
                    members.len(),
                )
            };
            hubs.sort_by(|a, b| {
                class(b)
                    .cmp(&class(a))
                    .then_with(|| g.key(a.id).cmp(g.key(b.id)))
            });
        }
    }
    let total = hubs.len();
    let start = range.start;
    let mut rows = Vec::new();
    for hub in hubs
        .into_iter()
        .skip(start)
        .take(range.end.saturating_sub(start))
    {
        check(cancel)?;
        let mut row = g.node_json(hub.id, cancel)?;
        if let Some(rank) = hub.rank {
            row.insert(
                "pageRank".into(),
                json!(super::super::detect::significant(rank)),
            );
        }
        row.insert("fanIn".into(), json!(hub.fan_in));
        row.insert("fanOut".into(), json!(hub.fan_out));
        row.insert("sites".into(), json!(hub.sites));
        rows.push(Value::Object(row));
    }
    Ok(Page { total, rows, start })
}
