//! Single-node answers: stats, node and symbols.
use super::*;

/// The snapshot's size, coverage and build facts. Its rankings are `hubs`
/// rows, one call away: `related` offers one call whose rows rank the most
/// imported files and packages (`hubs`) and the most called symbols (`hubs`
/// over `calls`).
///
/// `view:"summary"` (the default) is what decides the next call: size,
/// import and call linkage, languages and how many diagnostics there are.
/// `view:"details"` adds the breakdowns behind those numbers, the build's
/// own measurements and what the rankings read (`callConfidence`,
/// `testEdges`); the summary's `related` offers that call once.
pub(super) fn stats(g: &Graph, options: &QueryOptions) -> Result<GraphOutput, GraphOutput> {
    let m = &g.manifest;
    let details = options.view.as_deref() == Some(STATS_DETAILS);
    let row = |operation: &str, field: Option<(&str, Value)>| {
        let mut row = Map::new();
        row.insert("operation".into(), json!(operation));
        super::paging::pin(&mut row, options, &g.id);
        if let Some((name, value)) = field {
            row.insert(name.into(), value);
        }
        Value::Object(row)
    };
    // The rankings `hubs` answers read production edges only, calls at the
    // default confidence floor: what the calls ranking reads.
    let ranked_calls = EdgeFilter {
        kinds: vec![EdgeKind::Calls],
        weakest: DEFAULT_CONFIDENCE,
        tests: true,
    }
    .production();
    let calls = json!(
        ranked_calls
            .kinds
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>()
    );
    // One call answers both rankings: the most imported files and packages
    // (`hubs`, imports by default) and the most called symbols.
    let mut related = vec![
        crate::response::result::Continuation::related_input(
            crate::id::ToolId::GraphQuery,
            json!({"queries": [row("hubs", None), row("hubs", Some(("edges", calls)))]}),
        )
        .build(),
    ];
    let mut answer = json!({
        "graph": g.summary(),
        "operation": "stats",
        "octocodeVersion": m["octocodeVersion"],
        "scan": super::super::manifest::scan_disclosure(&m["scan"]),
        "counts": m["counts"],
        "imports": m["imports"],
        "calls": m["calls"],
        "argumentCoverage": m["argumentCoverage"],
        "linkage": m["linkage"],
        "linkageByLanguage": m["linkageByLanguage"],
        "extraction": m["extraction"],
        "languages": m["languages"],
        "diagnostics": m["diagnostics"],
    });
    if !details {
        if let Some(fields) = answer.as_object_mut() {
            fields.retain(|field, _| !STATS_DETAIL_FIELDS.contains(&field.as_str()));
        }
        for (holder, kept) in STATS_SUMMARY_PARTS {
            if let Some(whole) = answer[holder].as_object_mut() {
                whole.retain(|key, _| kept.contains(&key.as_str()));
            }
        }
        related.push(
            crate::response::result::Continuation::related(
                crate::id::ToolId::GraphQuery,
                row("stats", Some(("view", json!(STATS_DETAILS)))),
            )
            .build(),
        );
    } else {
        answer["callConfidence"] = json!(ranked_calls.weakest.as_str());
        if !ranked_calls.tests {
            answer["testEdges"] = json!("excluded");
        }
    }
    answer["related"] = Value::Array(related);
    Ok(GraphOutput::ok(answer))
}

/// The `view` value that asks `stats` for everything the manifest measured.
const STATS_DETAILS: &str = "details";
/// Manifest sections only `view:"details"` states.
const STATS_DETAIL_FIELDS: [&str; 4] = [
    "octocodeVersion",
    "argumentCoverage",
    "linkageByLanguage",
    "extraction",
];
/// Sections the summary states in part: the totals stay, their breakdowns
/// (`calls.byResolution`, `calls.unresolvedByReason`, `diagnostics.byCode`)
/// are details.
const STATS_SUMMARY_PARTS: [(&str, &[&str]); 2] = [
    ("calls", &["sites", "unresolved"]),
    ("diagnostics", &["total"]),
];

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
        // A point answer's keys come from lookup entries: the parent's is
        // checked against its node record before it is rendered.
        g.node_record(record.parent, cancel)?;
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
    let mut window = Window::budgeted(range, g.row_budget());
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
