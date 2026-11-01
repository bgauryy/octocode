//! The impact report: affected rows, their order and the summary.
use super::*;

/// One edge a change flows back along.
pub(super) struct Step {
    pub(super) src: u32,
    pub(super) kind: EdgeKind,
    pub(super) detail: u32,
    pub(super) confidence: Confidence,
    pub(super) types_only: bool,
}

/// Affected files as rows, sorted by distance (each carries its `depth` and
/// `risk`; `byDepth` counts them), plus the summary lists.
pub(super) fn report(
    t: &GraphTables,
    reach: &[Option<Reach>],
    seeds: &[u32],
    global: &[(String, String)],
    symbol_mode: bool,
    depth_limited: bool,
) -> Impact {
    let key = |id: u32| t.str(t.nodes[id as usize].key);
    let mut files = t
        .node_ids()
        .filter(|id| t.nodes[*id as usize].kind == NodeKind::File)
        .filter_map(|id| reach[id as usize].map(|r| (id, r)))
        .collect::<Vec<_>>();
    files.sort_by(|a, b| (a.1.depth, key(a.0)).cmp(&(b.1.depth, key(b.0))));
    let component_of = t
        .components
        .iter()
        .map(|(node, _, name, _)| (*node, t.str(*name)))
        .collect::<BTreeMap<_, _>>();
    let mut by_depth = BTreeMap::<u32, usize>::new();
    let mut entries = Vec::new();
    let mut tests = Vec::new();
    let mut components = BTreeSet::new();
    let mut rows = Vec::new();
    for (id, r) in &files {
        let flags = t.nodes[*id as usize].flags;
        *by_depth.entry(r.depth).or_default() += 1;
        if flags & ROLE_ENTRY != 0 {
            entries.push(key(*id));
        }
        if flags & ROLE_TEST != 0 {
            tests.push(key(*id));
        }
        if let Some(name) = component_of.get(id) {
            components.insert(*name);
        }
        rows.push(impact_row(t, *id, r));
    }
    // Test functions reached (inline Rust test modules, pytest functions,
    // functions in test files): the precise tests to run.
    let mut test_functions = t
        .node_ids()
        .filter(|id| {
            let node = &t.nodes[*id as usize];
            node.kind == NodeKind::Symbol
                && node.flags & FLAG_TEST != 0
                && reach[*id as usize].is_some()
        })
        .map(|id| (reach[id as usize].map_or(0, |r| r.depth), key(id)))
        .collect::<Vec<_>>();
    test_functions.sort_unstable();
    for (_, test) in &test_functions {
        let file = test.split('#').next().unwrap_or(test);
        if !tests.contains(&file) {
            tests.push(file);
        }
    }
    let seed_keys = seeds.iter().map(|id| key(*id)).collect::<Vec<_>>();
    let summary = json!({
        "changed": seed_keys,
        "configChanges": global.iter().map(|(dir, file)| json!({"file": file, "scope": if dir.is_empty() { "." } else { dir.as_str() }})).collect::<Vec<_>>(),
        "precision": if symbol_mode && global.is_empty() { "symbol" } else { "file" },
        "allAffected": global.iter().any(|(dir, _)| dir.is_empty()),
        "byDepth": by_depth,
        "affectedEntrypoints": entries,
        "testsToRun": tests,
        "testFunctions": test_functions.iter().map(|(_, k)| *k).collect::<Vec<_>>(),
        "components": components,
        "depthLimited": depth_limited,
    });
    Impact { summary, rows }
}

/// One affected file: its distance, risk bucket, and the edge that reached it.
pub(super) fn impact_row(t: &GraphTables, id: u32, r: &Reach) -> Value {
    let key = |id: u32| t.str(t.nodes[id as usize].key);
    let flags = t.nodes[id as usize].flags;
    // GitNexus-style buckets: d1 will break, d2 likely affected, d3+ test;
    // a module-level candidate binds no linked changed name, so it is no
    // breakage until a reference proves it.
    let risk = match r.depth {
        _ if r.candidate => "candidate",
        0 => "changed",
        1 => "direct",
        2 => "likely",
        _ => "transitive",
    };
    let mut row = json!({
        "id": key(id),
        "depth": r.depth,
        "risk": risk,
        "confidence": r.confidence.as_str(),
    });
    if r.package_depth > 0 {
        row["packageDistance"] = json!(r.package_depth);
    }
    match r.via {
        None => {
            row["via"] = json!("changed");
        }
        Some((from, kind, detail)) => {
            row["from"] = json!(key(from));
            row["via"] = json!(if detail == NONE {
                kind.as_str().to_owned()
            } else {
                format!("{}:{}", kind.as_str(), t.str(detail))
            });
        }
    }
    if r.types_only {
        row["typesOnly"] = json!(true);
    }
    let roles = role_names(flags);
    if !roles.is_empty() {
        row["roles"] = json!(roles);
    }
    row
}
