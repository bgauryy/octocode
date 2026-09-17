use super::{algorithms::*, build::normalize, types::*};
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::Path,
};

pub(crate) fn analyze(
    mut b: BuiltGraph,
    q: &AstGraphQuery,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
    let mut warnings = Vec::new();
    if b.truncated {
        warnings.push(format!(
            "scan stopped at maxFiles ({}) — graph results are partial",
            q.max_files.unwrap_or(20_000)
        ))
    }
    if b.files_skipped > 0 {
        warnings.push(format!("{} file(s) could not be read or parsed within the native graph bounds — graph results are partial",b.files_skipped))
    }
    let mut base = Map::new();
    base.insert("operation".into(), json!("topology"));
    base.insert("path".into(), json!(b.display_path));
    base.insert("filesScanned".into(), json!(b.facts.len()));
    base.insert("filesSkipped".into(), json!(b.files_skipped));
    let (items, summary, extra_warnings, low) = match q.analysis {
        GraphAnalysis::Dependencies | GraphAnalysis::Dependents => traversal(&b, q)?,
        GraphAnalysis::Path => path_analysis(&b, q)?,
        GraphAnalysis::Cycles => cycles(&b),
        GraphAnalysis::Reachability => reachability(&b, q, security),
        GraphAnalysis::DeadCode => dead_code(&b, q, security),
        GraphAnalysis::Drift => {
            return Err(AstGraphError::new(
                "invalidGraphQuery",
                "drift is dispatched before analyze",
            ));
        }
    };
    warnings.extend(extra_warnings);
    let (page, pagination, limit_truncated, total) = paginate(items, q);
    base.insert("results".into(), Value::Array(page));
    base.insert("pagination".into(), pagination);
    base.insert("summary".into(), summary);
    if !warnings.is_empty() {
        base.insert("warnings".into(), json!(warnings));
    }
    if low {
        base.insert("confidence".into(), json!("low"));
    }
    let mut reasons = Vec::<String>::new();
    if limit_truncated {
        reasons.push("limit".into());
        base.insert("totalAvailable".into(), json!(total));
    }
    if b.truncated {
        reasons.push("maxFiles".into());
    }
    if b.files_skipped > 0 {
        reasons.push("filesSkipped".into());
    }
    let has_parse = b.diagnostics.iter().any(|d| d.code == "parse-recovery");
    let has_unresolved = b
        .diagnostics
        .iter()
        .any(|d| d.code == "unresolved-internal");
    let has_unsupported = b
        .diagnostics
        .iter()
        .any(|d| d.code == "unsupported-linking");
    if has_parse {
        reasons.push("parseRecovery".into())
    }
    if has_unresolved {
        reasons.push("unresolvedImports".into())
    }
    if has_unsupported {
        reasons.push("unsupportedLinking".into())
    }
    reasons.sort_by_key(|x| {
        [
            "limit",
            "maxFiles",
            "filesSkipped",
            "parseRecovery",
            "unresolvedImports",
            "unsupportedLinking",
            "diagnosticPage",
        ]
        .iter()
        .position(|y| y == x)
        .unwrap_or(99)
    });
    reasons.dedup();
    let diagnostics_changed = add_coverage(&mut base, &mut b, q, &mut reasons);
    if !reasons.is_empty() {
        base.insert("truncated".into(), json!(true));
        base.insert("partialReasons".into(), json!(reasons));
    }
    let terminal = b.files_skipped > 0
        || has_parse
        || has_unsupported
        || (has_unresolved && !b.truncated)
        || q.max_files.is_some_and(|x| x >= 50_000) && b.truncated
        || q.limit.is_some_and(|x| x >= 5_000) && limit_truncated;
    if terminal {
        base.insert("terminalLimit".into(), json!(true));
    }
    if diagnostics_changed {
        let mut restart_query = clean_query(q);
        restart_query["diagnosticPage"] = json!(1);
        if let Some(query) = restart_query.as_object_mut() {
            query.remove("diagnosticSnapshot");
        }
        base.insert(
            "next".into(),
            json!({"restartDiagnostics":{"tool":"astSearch","query":restart_query,"why":"Restart diagnostic pagination from the current diagnostic snapshot.","confidence":"exact"}}),
        );
    } else {
        add_next(&mut base, q, limit_truncated, b.truncated, terminal);
    }
    let result_state = if base["pagination"]["hasMore"] == true {
        "pageable"
    } else if reasons
        .iter()
        .any(|x| matches!(x.as_str(), "limit" | "maxFiles" | "filesSkipped"))
    {
        "truncated"
    } else {
        "complete"
    };
    let gaps = reasons
        .iter()
        .filter_map(|x| match x.as_str() {
            "parseRecovery" => Some("parseRecovery"),
            "unresolvedImports" => Some("unresolvedImports"),
            "unsupportedLinking" => Some("unsupportedLinking"),
            _ => None,
        })
        .collect::<Vec<_>>();
    let graph_state = if reasons
        .iter()
        .any(|x| matches!(x.as_str(), "maxFiles" | "filesSkipped"))
    {
        "scan-truncated"
    } else if !gaps.is_empty() {
        "coverage-incomplete"
    } else {
        "complete"
    };
    let diag_state = if base["coverage"]["diagnosticsPagination"]["terminalLimit"] == true {
        "truncated"
    } else if base["coverage"]["diagnosticsPagination"]["hasMore"] == true {
        "pageable"
    } else {
        "complete"
    };
    base.insert("completeness".into(),if gaps.is_empty(){json!({"results":result_state,"graph":graph_state,"diagnostics":diag_state})}else{json!({"results":result_state,"graph":graph_state,"diagnostics":diag_state,"coverageGapReasons":gaps})});
    base.insert("analysis".into(), json!(q.analysis.as_str()));
    Ok(Value::Object(base))
}

fn traversal(
    b: &BuiltGraph,
    q: &AstGraphQuery,
) -> Result<(Vec<Value>, Value, Vec<String>, bool), AstGraphError> {
    let raw = q.file.as_deref().ok_or_else(|| {
        AstGraphError::new(
            "invalidGraphQuery",
            format!("{} requires file", q.analysis.as_str()),
        )
    })?;
    let file = graph_file(raw, &b.root);
    if !b.nodes.contains_key(&file) {
        return Err(AstGraphError::new(
            "invalidGraphQuery",
            format!("file is not in the scanned graph: {file}"),
        ));
    }
    let graph = if q.analysis == GraphAnalysis::Dependencies {
        b.nodes.clone()
    } else {
        reverse(&b.nodes)
    };
    let c = condense(&graph);
    let layers = layer_map(&c);
    let trans = find_transitive(&c);
    let indegree = in_degree(&b.nodes);
    let depth = q.depth.unwrap_or(1);
    let idoms = (depth > 1).then(|| dominators(&graph, &file));
    let mut items = traverse(&graph, &file, depth);
    for item in &mut items {
        let Some(f) = item["file"].as_str().map(str::to_owned) else {
            continue;
        };
        let Some(via) = item["via"].as_str().map(str::to_owned) else {
            continue;
        };
        let (importer, imported) = if q.analysis == GraphAnalysis::Dependencies {
            (via.as_str(), f.as_str())
        } else {
            (f.as_str(), via.as_str())
        };
        if let Some(line) = first_import_line(b, importer, imported) {
            item["importLine"] = json!(line)
        }
        item["inboundCount"] = json!(indegree.get(&f).copied().unwrap_or(0));
        item["immediateDominator"] = json!(if depth == 1 {
            Some(file.clone())
        } else {
            idoms.as_ref().and_then(|d| d.get(&f).cloned().flatten())
        });
        let a = c.component.get(&via).copied();
        let z = c.component.get(&f).copied();
        if let Some(z) = z {
            item["topologicalLayer"] = json!(layers.get(&z));
        }
        item["transitiveEdge"] = json!(a.zip(z).is_some_and(|e| trans.contains(&e)));
    }
    let summary = json!({"source":file,"depth":depth,"condensationComponentCount":c.components.len(),"topologicalLayerCount":c.layers.len(),"transitiveEdgeCount":trans.len()});
    Ok((items, summary, vec![], false))
}

fn path_analysis(
    b: &BuiltGraph,
    q: &AstGraphQuery,
) -> Result<(Vec<Value>, Value, Vec<String>, bool), AstGraphError> {
    let file = graph_file(
        q.file.as_deref().ok_or_else(|| {
            AstGraphError::new("invalidGraphQuery", "path requires file and target")
        })?,
        &b.root,
    );
    let target = graph_file(
        q.target.as_deref().ok_or_else(|| {
            AstGraphError::new("invalidGraphQuery", "path requires file and target")
        })?,
        &b.root,
    );
    if !b.nodes.contains_key(&file) || !b.nodes.contains_key(&target) {
        return Err(AstGraphError::new(
            "invalidGraphQuery",
            "file and target must both be in the scanned graph",
        ));
    }
    Ok((
        vec![shortest_path(&b.nodes, &file, &target)],
        json!({"source":file,"target":target}),
        vec![],
        false,
    ))
}

fn cycles(b: &BuiltGraph) -> (Vec<Value>, Value, Vec<String>, bool) {
    let c = condense(&b.nodes);
    let layers = layer_map(&c);
    let trans = find_transitive(&c);
    let runtime = runtime_graph(&b.nodes);
    let runtime_cycles = scc(&runtime, true);
    let mut items = Vec::new();
    for (id, files) in c.components.iter().enumerate() {
        if files.len() == 1 && !b.nodes[&files[0]].edges.contains_key(&files[0]) {
            continue;
        }
        let members = files.iter().cloned().collect::<BTreeSet<_>>();
        let contained = runtime_cycles
            .iter()
            .filter(|x| x.iter().all(|f| members.contains(f)))
            .cloned()
            .collect::<Vec<_>>();
        let runtime_count = contained.len();
        let kinds = collect_kinds(&b.nodes, files);
        let outgoing = c
            .edges
            .get(&id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        let outgoing_count = outgoing.len();
        items.push(json!({"files":files,"size":files.len(),"edgeKinds":kinds,"runtimeCycle":runtime_count>0,"runtimeCycles":contained,"runtimeCycleCount":runtime_count,"cycleEdges":cycle_witness(&b.nodes,&members),"runtimeCycleEdges":cycle_witness(&runtime,&members),"componentId":id,"topologicalLayer":layers.get(&id),"outgoingComponents":outgoing,"outgoingComponentCount":outgoing_count,"confidence":"syntactic"}));
    }
    let rc = items.iter().filter(|x| x["runtimeCycle"] == true).count();
    let ce = c.edges.values().map(BTreeSet::len).sum::<usize>();
    let count = items.len();
    let summary = json!({"cycleCount":count,"runtimeCycleCount":rc,"condensationComponentCount":c.components.len(),"condensationEdgeCount":ce,"topologicalLayerCount":c.layers.len(),"transitiveEdgeCount":trans.len()});
    (items, summary, vec![], false)
}

fn reachability(
    b: &BuiltGraph,
    q: &AstGraphQuery,
    security: &ContentSecurity,
) -> (Vec<Value>, Value, Vec<String>, bool) {
    let (roots, warnings, low) = entrypoints(b, q, security);
    if low && roots.is_empty() {
        return (
            vec![],
            json!({"entrypointsResolved":roots,"entrypointsResolvedCount":0,"classifiedCount":0,"unclassifiedCount":b.nodes.len()}),
            warnings,
            true,
        );
    }
    let live = reachable(&b.nodes, &roots, false);
    let items = b
        .nodes
        .keys()
        .map(|f| json!({"file":f,"reachable":live.contains(f),"confidence":"syntactic"}))
        .collect::<Vec<_>>();
    (
        items,
        json!({"entrypointsResolved":roots,"entrypointsResolvedCount":roots.len(),"reachableCount":live.len(),"unreachableCount":b.nodes.len()-live.len()}),
        warnings,
        low,
    )
}

fn dead_code(
    b: &BuiltGraph,
    q: &AstGraphQuery,
    security: &ContentSecurity,
) -> (Vec<Value>, Value, Vec<String>, bool) {
    let (roots, mut warnings, low_entries) = entrypoints(b, q, security);
    let live = reachable(&b.nodes, &roots, false);
    let static_live = reachable(&b.nodes, &roots, true);
    let dynamic = live.difference(&static_live).cloned().collect::<Vec<_>>();
    if !dynamic.is_empty() {
        warnings.push(format!("{} file(s) reachable only through a dynamic import() — lower confidence than static analysis, verify with lspSearch before treating as proof: {}",dynamic.len(),dynamic.join(", ")))
    }
    let rootset = roots.iter().cloned().collect::<BTreeSet<_>>();
    let public = rootset
        .union(&b.namespace_targets)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut real = BTreeSet::new();
    let mut rex: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for f in &live {
        if let Some(ff) = b.facts.get(f) {
            for i in &ff.imports {
                if let Some(t) = &i.target {
                    real.insert(binding(t, &i.imported_name));
                }
            }
            for r in &ff.reexports {
                if let Some(t) = &r.target {
                    rex.entry(binding(t, &r.imported_name))
                        .or_default()
                        .push((f.clone(), r.local_name.clone()));
                }
            }
        }
    }
    let star: BTreeMap<String, Vec<String>> = b
        .star_reexporters
        .iter()
        .filter_map(|(t, rs)| {
            let v = rs
                .iter()
                .filter(|x| live.contains(*x))
                .cloned()
                .collect::<Vec<_>>();
            (!v.is_empty()).then_some((t.clone(), v))
        })
        .collect();
    let star_targets = star.keys().cloned().collect::<BTreeSet<_>>();
    let components = scc_unsorted(&b.nodes);
    let mut cluster_by = BTreeMap::new();
    let mut clusters = Vec::new();
    for files in components {
        let report = files
            .into_iter()
            .filter(|f| q.include_tests.unwrap_or(true) || !is_test(f))
            .collect::<Vec<_>>();
        if report.is_empty() || !report.iter().all(|f| !live.contains(f)) {
            continue;
        }
        let id = clusters.len();
        for f in &report {
            cluster_by.insert(f.clone(), id);
        }
        clusters.push(json!({"id":id,"files":report,"reason":"mutually-referencing cluster with no path from any entrypoint — each file looks locally referenced by the others, but the cluster as a whole is unreachable","size":report.len(),"edgeKinds":collect_kinds(&b.nodes,&report),"confidence":"syntactic"}));
    }
    let mut rows = Vec::new();
    for (file, ff) in &b.facts {
        if !q.include_tests.unwrap_or(true) && is_test(file) {
            continue;
        }
        let live_names = live_names(file, ff, &public, &real, &rex, &star);
        for d in ff.declarations.iter().filter(|d| d.exported) {
            if !live.contains(file) {
                if let Some(id) = cluster_by.get(file) {
                    rows.push(json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"dead-cluster","clusterId":id}));
                } else {
                    rows.push(json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"unreachable-file"}));
                }
            } else if !rootset.contains(file)
                && !star_targets.contains(file)
                && !b.namespace_targets.contains(file)
                && !live_names.contains(&d.name)
            {
                rows.push(json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"unreferenced-export","viaHeuristic":if rex.contains_key(&binding(file,&d.name)){"reexport-chain"}else{"lexical-count"}}));
            }
        }
    }
    let count = rows.len();
    let ccount = clusters.len();
    (
        rows,
        json!({"entrypointsResolved":roots,"entrypointsResolvedCount":roots.len(),"deadClusters":clusters,"deadClusterCount":ccount,"deadExportCount":count}),
        warnings,
        low_entries
            || b.truncated
            || b.files_skipped > 0
            || b.diagnostics.iter().any(|x| x.code != "syntax-only"),
    )
}

fn binding(f: &str, n: &str) -> String {
    format!("{f}::{n}")
}
fn live_names(
    file: &str,
    ff: &FileFacts,
    public: &BTreeSet<String>,
    real: &BTreeSet<String>,
    rex: &BTreeMap<String, Vec<(String, String)>>,
    star: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    fn consumed(
        file: &str,
        name: &str,
        public: &BTreeSet<String>,
        real: &BTreeSet<String>,
        rex: &BTreeMap<String, Vec<(String, String)>>,
        star: &BTreeMap<String, Vec<String>>,
    ) -> bool {
        let mut seen = BTreeSet::new();
        let mut pending = vec![(file.to_owned(), name.to_owned())];
        while let Some((f, n)) = pending.pop() {
            let k = binding(&f, &n);
            if !seen.insert(k.clone()) {
                continue;
            }
            if public.contains(&f) || real.contains(&k) {
                return true;
            }
            pending.extend(rex.get(&k).cloned().unwrap_or_default());
            pending.extend(
                star.get(&f)
                    .into_iter()
                    .flatten()
                    .map(|x| (x.clone(), n.clone())),
            )
        }
        false
    }
    let exported = ff
        .declarations
        .iter()
        .filter(|d| d.exported)
        .map(|d| d.name.clone())
        .collect::<BTreeSet<_>>();
    let mut live = exported
        .iter()
        .filter(|n| consumed(file, n, public, real, rex, star))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut calls: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut exported_counts: BTreeMap<String, u32> = BTreeMap::new();
    for c in &ff.calls {
        calls
            .entry(c.caller.clone())
            .or_default()
            .push(c.callee.clone());
        if exported.contains(&c.caller) {
            *exported_counts.entry(c.callee.clone()).or_default() += 1
        }
    }
    let mut pending = live.iter().cloned().collect::<VecDeque<_>>();
    for (c, targets) in &calls {
        if !exported.contains(c) {
            for t in targets {
                if exported.contains(t) && live.insert(t.clone()) {
                    pending.push_back(t.clone())
                }
            }
        }
    }
    for n in &exported {
        if ff
            .reference_counts
            .get(n)
            .copied()
            .unwrap_or(0)
            .saturating_sub(exported_counts.get(n).copied().unwrap_or(0))
            > 1
            && live.insert(n.clone())
        {
            pending.push_back(n.clone())
        }
    }
    while let Some(c) = pending.pop_front() {
        for t in calls.get(&c).into_iter().flatten() {
            if t != &c && exported.contains(t) && live.insert(t.clone()) {
                pending.push_back(t.clone())
            }
        }
    }
    live
}

fn entrypoints(
    b: &BuiltGraph,
    q: &AstGraphQuery,
    security: &ContentSecurity,
) -> (Vec<String>, Vec<String>, bool) {
    let mut roots = Vec::new();
    let mut seen_roots = BTreeSet::new();
    let mut warnings = Vec::new();
    let mut low = false;
    if let Some(explicit) = q.entrypoints.as_ref().filter(|x| !x.is_empty()) {
        for raw in explicit {
            let p = if Path::new(raw).is_absolute() {
                Path::new(raw)
                    .strip_prefix(&b.root)
                    .ok()
                    .map(|x| normalize(&x.to_string_lossy()))
                    .unwrap_or_else(|| normalize(raw))
            } else {
                normalize(raw)
            };
            if b.nodes.contains_key(&p) {
                push_unique(&mut roots, &mut seen_roots, p);
            } else {
                let prefix = if p == "." {
                    "".into()
                } else {
                    format!("{}/", p.trim_end_matches('/'))
                };
                let found = b
                    .nodes
                    .keys()
                    .filter(|x| x.starts_with(&prefix))
                    .cloned()
                    .collect::<Vec<_>>();
                if found.is_empty() {
                    warnings.push(format!("entrypoint not found in scan: {raw} — pass a file or dynamic asset directory relative to the scanned path, or an absolute path under it"))
                } else {
                    for file in found {
                        push_unique(&mut roots, &mut seen_roots, file);
                    }
                }
            }
        }
    } else {
        let pkg = b.root.join("package.json");
        if let Ok(bytes) = fs::read(&pkg)
            && let Ok(safe) = security.validate_text_bytes(&bytes, Some(&pkg), 1_000_000)
            && let Ok(v) = serde_json::from_str::<Value>(&safe.content)
        {
            let mut leaves = Vec::new();
            for k in ["main", "exports", "bin"] {
                leaves_json(&v[k], &mut leaves)
            }
            if let Some(s) = v["scripts"].as_object() {
                for value in s.values().filter_map(Value::as_str) {
                    for token in value.split_whitespace() {
                        let t = token
                            .split('=')
                            .next_back()
                            .unwrap_or(token)
                            .trim_matches(['\"', '\'']);
                        if [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".mts", ".cts"]
                            .iter()
                            .any(|x| t.ends_with(x))
                        {
                            leaves.push(t.into())
                        }
                    }
                }
            }
            for leaf in leaves {
                if let Some(x) =
                    source_equivalent(&normalize(leaf.trim_start_matches("./")), &b.nodes)
                {
                    push_unique(&mut roots, &mut seen_roots, x);
                }
            }
        }
        if roots.is_empty() {
            warnings.push("no entrypoints resolved from package.json — pass `entrypoints` explicitly, or every export in a reachable file will read as unreachable".into());
            low = true
        }
    }
    if q.include_tests.unwrap_or(true) {
        for file in b.nodes.keys().filter(|x| is_test(x)).cloned() {
            push_unique(&mut roots, &mut seen_roots, file);
        }
    }
    (roots, warnings, low)
}
fn push_unique(roots: &mut Vec<String>, seen: &mut BTreeSet<String>, file: String) {
    if seen.insert(file.clone()) {
        roots.push(file);
    }
}
fn leaves_json(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(x) => out.push(x.clone()),
        Value::Array(a) => {
            for x in a {
                leaves_json(x, out)
            }
        }
        Value::Object(m) => {
            for x in m.values() {
                leaves_json(x, out)
            }
        }
        _ => {}
    }
}
fn source_equivalent(p: &str, nodes: &BTreeMap<String, Node>) -> Option<String> {
    if nodes.contains_key(p) {
        return Some(p.into());
    }
    let mut c = Vec::new();
    for (a, b) in [
        (".js", ".ts"),
        (".jsx", ".tsx"),
        (".mjs", ".mts"),
        (".cjs", ".cts"),
    ] {
        if let Some(s) = p.strip_suffix(a) {
            c.push(format!("{s}{b}"));
        }
    }
    for d in ["dist/", "build/", "out/"] {
        if let Some(rest) = p.strip_prefix(d) {
            c.push(format!("src/{rest}"));
            for (a, b) in [
                (".js", ".ts"),
                (".jsx", ".tsx"),
                (".mjs", ".mts"),
                (".cjs", ".cts"),
            ] {
                if let Some(s) = rest.strip_suffix(a) {
                    c.push(format!("src/{s}{b}"));
                }
            }
            if nodes.contains_key("src/index.ts") {
                c.push("src/index.ts".into())
            }
        }
    }
    c.into_iter().find(|x| nodes.contains_key(x))
}
fn is_test(f: &str) -> bool {
    let l = f.to_ascii_lowercase();
    l.contains("/__tests__/")
        || l.starts_with("tests/")
        || l.contains("/tests/")
        || [".test.", ".spec."].iter().any(|x| l.contains(x))
}

/// Compare the import topology of a baseline root against the head `path` and
/// report typed structural drift. Builds two independent snapshots and diffs
/// them through the shared engine so results stay AST-only and deterministic.
pub(crate) fn drift(
    q: &AstGraphQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    let head = super::build::build_graph(q, paths, security, cancel)?;
    let baseline_root = q
        .baseline
        .clone()
        .ok_or_else(|| AstGraphError::new("invalidGraphQuery", "drift requires baseline"))?;
    let mut base_query = q.clone();
    base_query.path = Some(baseline_root);
    base_query.baseline = None;
    let mut base = super::build::build_graph(&base_query, paths, security, cancel)?;
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;

    // The native scanner keys every node on a path relative to its own scan
    // root, so two-root drift shares identity across differing absolute roots.
    // Equalize the root metadata so the engine's root-mismatch gate — which
    // protects consumers that key on absolute paths — does not false-refuse this
    // legitimately comparable pair. The real roots stay visible in `path` /
    // `baseline` below.
    base.code_graph.snapshot.root = head.code_graph.snapshot.root.clone();
    let diff = octocode_engine::graph::diff_graphs(&base.code_graph, &head.code_graph);

    let mut items: Vec<Value> = Vec::new();
    for rel in &diff.relations.added {
        items.push(json!({"category":"relation","change":"added","from":rel.from.0,"to":rel.to.0,"edgeKind":format!("{:?}",rel.kind),"confidence":"syntactic"}));
    }
    for rel in &diff.relations.removed {
        items.push(json!({"category":"relation","change":"removed","from":rel.from.0,"to":rel.to.0,"edgeKind":format!("{:?}",rel.kind),"confidence":"syntactic"}));
    }
    for rel in &diff.relations.static_to_dynamic {
        items.push(json!({"category":"transition","change":"staticToDynamic","from":rel.from.0,"to":rel.to.0,"confidence":"syntactic"}));
    }
    for rel in &diff.relations.dynamic_to_static {
        items.push(json!({"category":"transition","change":"dynamicToStatic","from":rel.from.0,"to":rel.to.0,"confidence":"syntactic"}));
    }
    for cycle in &diff.cycles.added {
        items.push(
            json!({"category":"cycle","change":"added","files":cycle,"confidence":"syntactic"}),
        );
    }
    for cycle in &diff.cycles.resolved {
        items.push(
            json!({"category":"cycle","change":"resolved","files":cycle,"confidence":"syntactic"}),
        );
    }
    for file in &diff.files.added {
        items.push(json!({"category":"file","change":"added","file":file}));
    }
    for file in &diff.files.removed {
        items.push(json!({"category":"file","change":"removed","file":file}));
    }
    for file in &diff.files.changed {
        items.push(json!({"category":"file","change":"changed","file":file}));
    }

    let summary = json!({
        "comparable": diff.comparable,
        "incompatibilities": serde_json::to_value(&diff.incompatibilities).unwrap_or(Value::Null),
        "relationsAdded": diff.relations.added.len(),
        "relationsRemoved": diff.relations.removed.len(),
        "staticToDynamic": diff.relations.static_to_dynamic.len(),
        "dynamicToStatic": diff.relations.dynamic_to_static.len(),
        "cyclesAdded": diff.cycles.added.len(),
        "cyclesResolved": diff.cycles.resolved.len(),
        "filesAdded": diff.files.added.len(),
        "filesRemoved": diff.files.removed.len(),
        "filesChanged": diff.files.changed.len(),
        "completeness": serde_json::to_value(&diff.completeness).unwrap_or(Value::Null),
        "metrics": serde_json::to_value(&diff.metrics).unwrap_or(Value::Null),
    });

    let mut base_map = Map::new();
    base_map.insert("operation".into(), json!("topology"));
    base_map.insert("analysis".into(), json!("drift"));
    base_map.insert("path".into(), json!(head.display_path));
    base_map.insert("baseline".into(), json!(base.display_path));
    base_map.insert("filesScanned".into(), json!(head.facts.len()));
    base_map.insert("baselineFilesScanned".into(), json!(base.facts.len()));

    let (page, pagination, limit_truncated, total) = paginate(items, q);
    let has_more = pagination["hasMore"] == json!(true);
    base_map.insert("results".into(), Value::Array(page));
    base_map.insert("pagination".into(), pagination);
    base_map.insert("summary".into(), summary);

    let mut reasons: Vec<&str> = Vec::new();
    if !diff.comparable {
        base_map.insert("confidence".into(), json!("low"));
        base_map.insert(
            "warnings".into(),
            json!(["graphs are not comparable — see summary.incompatibilities"]),
        );
    }
    if limit_truncated {
        base_map.insert("totalAvailable".into(), json!(total));
        reasons.push("limit");
    }
    if head.truncated || base.truncated {
        reasons.push("maxFiles");
    }
    if head.files_skipped > 0 || base.files_skipped > 0 {
        reasons.push("filesSkipped");
    }
    if !reasons.is_empty() {
        base_map.insert("truncated".into(), json!(true));
        base_map.insert("partialReasons".into(), json!(reasons));
    }
    let results_state = if has_more {
        "pageable"
    } else if reasons.is_empty() {
        "complete"
    } else {
        "truncated"
    };
    let graph_state =
        if head.truncated || base.truncated || head.files_skipped > 0 || base.files_skipped > 0 {
            "scan-truncated"
        } else {
            "complete"
        };
    base_map.insert(
        "completeness".into(),
        json!({"results":results_state,"graph":graph_state,"diagnostics":"complete"}),
    );
    if has_more && q.page < 1000 {
        base_map.insert(
            "next".into(),
            json!({"nextPage": continuation(q, Some(q.page + 1), None, "Continue topology drift results.")}),
        );
    }
    Ok(Value::Object(base_map))
}

fn paginate(items: Vec<Value>, q: &AstGraphQuery) -> (Vec<Value>, Value, bool, usize) {
    let total = items.len();
    let limited = if let Some(l) = q.limit {
        items.into_iter().take(l as usize).collect()
    } else {
        items
    };
    let truncated = limited.len() < total;
    let size = q.page_size.unwrap_or(50).clamp(1, 100) as usize;
    let pages = usize::max(1, limited.len().div_ceil(size));
    let current = (q.page.max(1) as usize).min(pages);
    let start = (current - 1) * size;
    let mut pagination = json!({
        "currentPage": current,
        "totalPages": pages,
        "entriesPerPage": size,
        "totalEntries": limited.len(),
        "hasMore": current < pages
    });
    if q.page as usize > pages {
        pagination["outOfRange"] = json!(true);
    }
    (
        limited.iter().skip(start).take(size).cloned().collect(),
        pagination,
        truncated,
        total,
    )
}

fn add_coverage(
    base: &mut Map<String, Value>,
    b: &mut BuiltGraph,
    q: &AstGraphQuery,
    reasons: &mut Vec<String>,
) -> bool {
    b.diagnostics.sort();
    b.diagnostics.dedup();
    let tuples = b
        .diagnostics
        .iter()
        .map(|d| json!([d.file, d.line, d.code, d.message]))
        .collect::<Vec<_>>();
    let id = hex::encode(Sha256::digest(
        serde_json::to_vec(&tuples).unwrap_or_default(),
    ));
    let mut counts = BTreeMap::<String, u32>::new();
    for d in &b.diagnostics {
        *counts.entry(d.code.clone()).or_default() += 1
    }
    let languages=b.languages.iter().map(|(language,files,linking)|json!({"language":language,"files":files,"linking":linking})).collect::<Vec<_>>();
    if q.diagnostic_snapshot.as_ref().is_some_and(|x| x != &id) {
        base.insert("status".into(), json!("error"));
        base.insert("errorCode".into(), json!("graphDiagnosticsChanged"));
        base.insert("error".into(), json!("Graph diagnostics changed between pages. Restart before combining diagnostic pages."));
        base.insert("results".into(), json!([]));
        base.insert("coverage".into(),json!({"basis":"syntactic","referenceBasis":"lexical-occurrence","languages":languages,"imports":{"resolved":b.imports[0],"external":b.imports[1],"unresolvedInternal":b.imports[2],"unsupported":b.imports[3]},"diagnostics":[],"diagnosticCounts":counts}));
        return true;
    }
    let size = q.diagnostic_page_size.unwrap_or(25).clamp(1, 100) as usize;
    let pages = usize::max(1, b.diagnostics.len().div_ceil(size));
    let current = (q.diagnostic_page.max(1) as usize).min(pages);
    let more = current < pages;
    if more {
        reasons.push("diagnosticPage".into())
    }
    let ds = b
        .diagnostics
        .iter()
        .skip((current - 1) * size)
        .take(size)
        .collect::<Vec<_>>();
    let mut diagnostics_pagination = json!({"currentPage":current,"totalPages":pages,"entriesPerPage":size,"totalEntries":b.diagnostics.len(),"hasMore":more,"resultId":id});
    if q.diagnostic_page as usize > pages {
        diagnostics_pagination["outOfRange"] = json!(true);
        let warning = format!(
            "diagnosticPage:{} is out of range; returned diagnostic page {}.",
            q.diagnostic_page, current
        );
        match base.get_mut("warnings") {
            Some(Value::Array(warnings)) => warnings.push(json!(warning)),
            _ => {
                base.insert("warnings".into(), json!([warning]));
            }
        }
    }
    base.insert("coverage".into(),json!({"basis":"syntactic","referenceBasis":"lexical-occurrence","languages":languages,"imports":{"resolved":b.imports[0],"external":b.imports[1],"unresolvedInternal":b.imports[2],"unsupported":b.imports[3]},"diagnostics":ds,"diagnosticCounts":counts,"diagnosticsPagination":diagnostics_pagination}));
    false
}
fn add_next(
    base: &mut Map<String, Value>,
    q: &AstGraphQuery,
    limit_truncated: bool,
    scan_truncated: bool,
    _terminal: bool,
) {
    let mut next = Map::new();
    if base["pagination"]["hasMore"] == true && q.page < 1000 {
        let why = match q.analysis {
            GraphAnalysis::Dependencies => "Continue dependencies.",
            GraphAnalysis::Dependents => "Continue dependents.",
            GraphAnalysis::Path => "Continue path results.",
            GraphAnalysis::Cycles => "Continue cycle components.",
            GraphAnalysis::Reachability => "Continue reachability classifications.",
            GraphAnalysis::DeadCode => "Continue dead-code candidates.",
            GraphAnalysis::Drift => "Continue topology drift results.",
        };
        next.insert(
            "nextPage".into(),
            continuation(q, Some(q.page + 1), None, why),
        );
    }
    if base["coverage"]["diagnosticsPagination"]["hasMore"] == true && q.diagnostic_page < 1000 {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(q.diagnostic_page + 1);
        value["diagnosticPageSize"] = json!(q.diagnostic_page_size.unwrap_or(25));
        value["diagnosticSnapshot"] = base["coverage"]["diagnosticsPagination"]["resultId"].clone();
        next.insert(
            "nextDiagnostics".into(),
            json!({"tool":"astSearch","query":value,"why":"Continue coverage diagnostics from the same diagnostic snapshot.","confidence":"exact"}),
        );
    }
    if base["coverage"]["diagnosticsPagination"]["outOfRange"] == true {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(1);
        if let Some(query) = value.as_object_mut() {
            query.remove("diagnosticSnapshot");
        }
        next.insert(
            "restartDiagnostics".into(),
            json!({"tool":"astSearch","query":value,"why":"Restart diagnostic pagination from the current diagnostic snapshot.","confidence":"exact"}),
        );
    }
    if scan_truncated && q.max_files.unwrap_or(20_000) < 50_000 {
        let cur = q.max_files.unwrap_or(20_000);
        next.insert(
            "expandScan".into(),
            continuation(
                q,
                Some(1),
                Some((cur * 2).max(cur + 1).min(50_000)),
                "Re-run with a larger file-scan bound because this graph is partial.",
            ),
        );
    }
    if let Some(limit) = q.limit.filter(|limit| *limit < 5_000)
        && limit_truncated
    {
        let mut value = clean_query(q);
        value["limit"] = json!((limit * 2).max(limit + 1).min(5_000));
        value["page"] = json!(1);
        value["diagnosticPage"] = json!(1);
        if let Some(query) = value.as_object_mut() {
            query.remove("diagnosticSnapshot");
        }
        next.insert("expandLimit".into(),json!({"tool":"astSearch","query":value,"why":"Re-run with a larger result limit because additional graph results exist.","confidence":"exact"}));
    }
    if q.analysis == GraphAnalysis::DeadCode
        && let Some(c) = base["results"].as_array().and_then(|x| x.first())
        && let (Some(file), Some(name), Some(line)) =
            (c["file"].as_str(), c["name"].as_str(), c["line"].as_u64())
    {
        let root = q.path.as_deref().unwrap_or("").trim_end_matches('/');
        next.insert("verifyReferences".into(),json!({"tool":"lspSearch","query":{"operation":"references","uri":format!("{root}/{file}"),"symbolName":name,"lineHint":line,"includeDeclaration":false,"groupByFile":true},"why":format!("Verify candidate \"{name}\" before deletion; repeat for each result, prioritizing viaHeuristic:\"reexport-chain\"."),"confidence":"high"}));
    }
    if !next.is_empty() {
        base.insert("next".into(), Value::Object(next));
    }
}
fn clean_query(q: &AstGraphQuery) -> Value {
    let mut v = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(m) = v.as_object_mut() {
        m.retain(|_, x| !x.is_null());
        if m.get("diagnosticPage") == Some(&json!(1)) {
            m.remove("diagnosticPage");
        }
        if matches!(
            q.analysis,
            GraphAnalysis::Reachability | GraphAnalysis::DeadCode
        ) && q.include_tests.is_none()
        {
            m.insert("includeTests".into(), json!(true));
        }
    }
    v
}
fn continuation(q: &AstGraphQuery, page: Option<u32>, max: Option<u32>, why: &str) -> Value {
    let mut v = clean_query(q);
    if let Some(x) = page {
        v["page"] = json!(x)
    }
    if let Some(x) = max {
        v["maxFiles"] = json!(x);
        if let Some(query) = v.as_object_mut() {
            query.remove("diagnosticSnapshot");
        }
        v["diagnosticPage"] = json!(1)
    }
    json!({"tool":"astSearch","query":v,"why":why,"confidence":"exact"})
}
fn graph_file(f: &str, root: &Path) -> String {
    let p = Path::new(f);
    if p.is_absolute() {
        p.strip_prefix(root)
            .ok()
            .map(|x| normalize(&x.to_string_lossy()))
            .unwrap_or_else(|| normalize(f))
    } else {
        normalize(f)
    }
}
fn collect_kinds(g: &BTreeMap<String, Node>, files: &[String]) -> Vec<String> {
    let members = files.iter().collect::<BTreeSet<_>>();
    let mut kinds = BTreeSet::new();
    for f in files {
        if let Some(n) = g.get(f) {
            for (t, k) in &n.edges {
                if members.contains(t) {
                    kinds.extend(k.iter().cloned())
                }
            }
        }
    }
    kinds.into_iter().collect()
}
fn runtime_graph(g: &BTreeMap<String, Node>) -> BTreeMap<String, Node> {
    let runtime = BTreeSet::from([
        "static-import",
        "dynamic-import",
        "named-reexport",
        "star-reexport",
        "commonjs-require",
        "create-require",
        "python-import",
    ]);
    g.iter()
        .map(|(f, n)| {
            let edges = n
                .edges
                .iter()
                .filter(|(_, k)| k.iter().any(|x| runtime.contains(x.as_str())))
                .map(|(t, k)| (t.clone(), k.clone()))
                .collect();
            (
                f.clone(),
                Node {
                    edges,
                    dynamic_only: n.dynamic_only.clone(),
                },
            )
        })
        .collect()
}
fn layer_map(c: &Condensed) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for (i, l) in c.layers.iter().enumerate() {
        for x in l {
            out.insert(*x, i);
        }
    }
    out
}
fn find_transitive(c: &Condensed) -> BTreeSet<(usize, usize)> {
    transitive_edges(&c.edges)
}
fn in_degree(g: &BTreeMap<String, Node>) -> BTreeMap<String, u32> {
    let mut d = BTreeMap::new();
    for n in g.values() {
        for t in n.edges.keys() {
            *d.entry(t.clone()).or_default() += 1
        }
    }
    d
}
fn first_import_line(b: &BuiltGraph, importer: &str, target: &str) -> Option<u32> {
    b.facts
        .get(importer)?
        .imports
        .iter()
        .find(|i| i.target.as_deref() == Some(target))
        .map(|i| i.line)
}
fn dominators(g: &BTreeMap<String, Node>, source: &str) -> BTreeMap<String, Option<String>> {
    struct Frame {
        node: String,
        successors: Vec<String>,
        offset: usize,
    }
    let mut seen = BTreeSet::from([source.to_owned()]);
    let mut postorder = Vec::new();
    let mut frames = vec![Frame {
        node: source.to_owned(),
        successors: g
            .get(source)
            .map(|node| node.edges.keys().cloned().collect())
            .unwrap_or_default(),
        offset: 0,
    }];
    while let Some(frame) = frames.last_mut() {
        if let Some(successor) = frame.successors.get(frame.offset).cloned() {
            frame.offset += 1;
            if seen.insert(successor.clone()) {
                frames.push(Frame {
                    node: successor.clone(),
                    successors: g
                        .get(&successor)
                        .map(|node| node.edges.keys().cloned().collect())
                        .unwrap_or_default(),
                    offset: 0,
                });
            }
            continue;
        }
        postorder.push(frames.pop().expect("frame exists").node);
    }
    postorder.reverse();
    let order: BTreeMap<String, usize> = postorder
        .iter()
        .enumerate()
        .map(|(index, node)| (node.clone(), index))
        .collect();
    let mut predecessors: BTreeMap<String, BTreeSet<String>> = postorder
        .iter()
        .map(|node| (node.clone(), BTreeSet::new()))
        .collect();
    for node in &postorder {
        for target in g.get(node).into_iter().flat_map(|node| node.edges.keys()) {
            if let Some(entries) = predecessors.get_mut(target) {
                entries.insert(node.clone());
            }
        }
    }
    let mut idom = BTreeMap::from([(source.to_owned(), source.to_owned())]);
    fn intersect(
        mut left: String,
        mut right: String,
        order: &BTreeMap<String, usize>,
        idom: &BTreeMap<String, String>,
    ) -> String {
        while left != right {
            while order[&left] > order[&right] {
                left = idom[&left].clone();
            }
            while order[&right] > order[&left] {
                right = idom[&right].clone();
            }
        }
        left
    }
    loop {
        let mut changed = false;
        for node in postorder.iter().skip(1) {
            let preds = predecessors[node]
                .iter()
                .filter(|pred| idom.contains_key(*pred))
                .cloned()
                .collect::<Vec<_>>();
            let Some(mut next) = preds.first().cloned() else {
                continue;
            };
            for pred in preds.iter().skip(1) {
                next = intersect(pred.clone(), next, &order, &idom);
            }
            if idom.get(node) != Some(&next) {
                idom.insert(node.clone(), next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut out = BTreeMap::from([(source.to_owned(), None)]);
    out.extend(
        postorder
            .into_iter()
            .skip(1)
            .map(|node| (node.clone(), idom.get(&node).cloned())),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(edges: &[&str]) -> Node {
        Node {
            edges: edges
                .iter()
                .map(|target| {
                    (
                        (*target).to_owned(),
                        BTreeSet::from(["static-import".to_owned()]),
                    )
                })
                .collect(),
            dynamic_only: BTreeSet::new(),
        }
    }

    #[test]
    fn immediate_dominators_use_the_complete_reachable_diamond() {
        let graph = BTreeMap::from([
            ("entry.ts".into(), node(&["left.ts", "right.ts"])),
            ("left.ts".into(), node(&["shared.ts"])),
            ("right.ts".into(), node(&["shared.ts"])),
            ("shared.ts".into(), node(&["deep.ts"])),
            ("deep.ts".into(), node(&[])),
        ]);
        let actual = dominators(&graph, "entry.ts");
        assert_eq!(actual["left.ts"].as_deref(), Some("entry.ts"));
        assert_eq!(actual["right.ts"].as_deref(), Some("entry.ts"));
        assert_eq!(actual["shared.ts"].as_deref(), Some("entry.ts"));
        assert_eq!(actual["deep.ts"].as_deref(), Some("shared.ts"));
    }
}
