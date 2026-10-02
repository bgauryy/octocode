use super::{algorithms::*, graph::normalize, types::*};
use crate::tools::id::ToolId;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use octocode_engine::graph::IMPORT_USE_MODULE;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::Path,
};

pub(crate) fn analyze(
    b: &mut BuiltGraph,
    q: &AstTopologyQuery,
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
            q.max_files().unwrap_or(20_000)
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
    let (items, mut summary, extra_warnings, mut low) = match q.analysis() {
        GraphAnalysis::Dependencies | GraphAnalysis::Dependents => traversal(b, q)?,
        GraphAnalysis::Path => path_analysis(b, q)?,
        GraphAnalysis::Cycles => cycles(b),
        GraphAnalysis::Reachability => reachability(b, q, security),
        GraphAnalysis::DeadCode => dead_code(b, q, security),
        GraphAnalysis::Drift => {
            return Err(AstGraphError::new(
                "invalidGraphQuery",
                "drift is dispatched before analyze",
            ));
        }
    };
    warnings.extend(extra_warnings);
    // The resolved root list is page-invariant: emit it with the first result
    // page only; later pages keep `entrypointsResolvedCount`.
    if q.page() > 1
        && let Some(obj) = summary.as_object_mut()
    {
        obj.remove("entrypointsResolved");
    }
    // Import-resolution health drives whether an edge-derived answer can be
    // trusted. Compute it before shaping the summary/confidence so an incomplete
    // import graph never presents as a confident zero (e.g. `cycleCount:0` when
    // no `crate::` import could be resolved). Only ever escalates to low — it
    // never downgrades a signal an analysis already marked low.
    let has_parse = b.diagnostics.iter().any(|d| d.code == "parse-recovery");
    let has_unresolved = b
        .diagnostics
        .iter()
        .any(|d| d.code == "unresolved-internal");
    let has_unsupported = b
        .diagnostics
        .iter()
        .any(|d| d.code == "unsupported-linking");
    if has_unresolved || has_unsupported {
        low = true;
        if let Some(obj) = summary.as_object_mut() {
            let resolved = b.imports[0];
            obj.insert(
                "importResolution".into(),
                json!({
                    "status": if resolved == 0 { "failed" } else { "partial" },
                    "resolved": resolved,
                    "unresolvedInternal": b.imports[2],
                    "unsupported": b.imports[3],
                }),
            );
        }
    }
    let results_digest = digest(&items);
    let (page, pagination, limit_truncated, total) = paginate(items, q);
    warnings.extend(out_of_range_warning(&pagination));
    base.insert("results".into(), Value::Array(page));
    base.insert("pagination".into(), pagination);
    base.insert("summary".into(), summary);
    if !warnings.is_empty() {
        base.insert("warnings".into(), json!(warnings));
    }
    if low {
        base.insert("confidence".into(), json!("low"));
    }
    // `reasons` names real scope cuts (result limit, file-scan bound, skipped
    // files). Coverage gaps — parse recovery, unresolved or unsupported
    // imports — are not truncation: every page is still reachable, so they
    // surface only as `completeness.coverageGapReasons` plus `confidence`.
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
    let gaps = [
        (has_parse, "parseRecovery"),
        (has_unresolved, "unresolvedImports"),
        (has_unsupported, "unsupportedLinking"),
    ]
    .into_iter()
    .filter_map(|(present, reason)| present.then_some(reason))
    .collect::<Vec<_>>();
    let coverage_state = add_coverage(&mut base, b, q, &results_digest);
    if !reasons.is_empty() {
        base.insert("truncated".into(), json!(true));
        base.insert("partialReasons".into(), json!(reasons));
    }
    let terminal = b.files_skipped > 0
        || q.max_files().is_some_and(|x| x >= 50_000) && b.truncated
        || q.limit().is_some_and(|x| x >= 5_000) && limit_truncated;
    if terminal {
        base.insert("terminalLimit".into(), json!(true));
    }
    if coverage_state.changed {
        base.insert(
            "next".into(),
            json!({"restartDiagnostics": restart_continuation(q)}),
        );
    } else {
        add_next(
            &mut base,
            q,
            &b.root,
            limit_truncated,
            b.truncated,
            coverage_state.withheld.as_deref(),
        );
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
    } else if base["coverage"]["diagnosticsPagination"]["hasMore"] == true
        || coverage_state.withheld.is_some()
    {
        "pageable"
    } else {
        "complete"
    };
    base.insert("completeness".into(),if gaps.is_empty(){json!({"results":result_state,"graph":graph_state,"diagnostics":diag_state})}else{json!({"results":result_state,"graph":graph_state,"diagnostics":diag_state,"coverageGapReasons":gaps})});
    base.insert("analysis".into(), json!(q.analysis().as_str()));
    Ok(Value::Object(base))
}

fn traversal(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
) -> Result<(Vec<Value>, Value, Vec<String>, bool), AstGraphError> {
    let raw = q.file().ok_or_else(|| {
        AstGraphError::new(
            "invalidGraphQuery",
            format!("{} requires file", q.analysis().as_str()),
        )
    })?;
    let file = node_key(raw, &b.root, &b.nodes);
    if !b.nodes.contains_key(&file) {
        return Err(missing_file_error(q, &file, &b.nodes));
    }
    let graph = if q.analysis() == GraphAnalysis::Dependencies {
        b.nodes.clone()
    } else {
        reverse(&b.nodes)
    };
    let c = condense(&graph);
    let layers = layer_map(&c);
    let trans = find_transitive(&c);
    let indegree = in_degree(&b.nodes);
    let depth = q.depth().unwrap_or(1);
    let idoms = (depth > 1).then(|| dominators(&graph, &file));
    let mut items = traverse(&graph, &file, depth);
    if q.analysis() == GraphAnalysis::Dependents {
        let listed = items
            .iter()
            .filter_map(|item| item["file"].as_str().map(str::to_owned))
            .collect::<BTreeSet<_>>();
        for (importer, module) in reexport_dependents(b, &file) {
            if importer == file || listed.contains(&importer) {
                continue;
            }
            let kinds = b
                .nodes
                .get(&importer)
                .and_then(|node| node.edges.get(&module))
                .cloned()
                .unwrap_or_default();
            items.push(json!({"file":importer,"distance":1,"via":module,"reexportVia":module,"edgeKinds":kinds,"confidence":"syntactic"}));
        }
    }
    for item in &mut items {
        let Some(f) = item["file"].as_str().map(str::to_owned) else {
            continue;
        };
        let Some(via) = item["via"].as_str().map(str::to_owned) else {
            continue;
        };
        let (importer, imported) = if q.analysis() == GraphAnalysis::Dependencies {
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

/// Files that use `target`'s items through a module re-exporting them:
/// `sync/mod.rs` has `pub use notify::Notify` and `sync/broadcast.rs` has
/// `use super::Notify`. A module re-exports what it imports or re-exports
/// from `target` (`export * from` and `pub use x::*` re-export every public
/// name); an importer of that module counts only when it names one of those
/// items, so importers of other items of a hub module are not dependents.
/// Returns `(importer, re-exporting module)` pairs in file order.
fn reexport_dependents(b: &BuiltGraph, target: &str) -> Vec<(String, String)> {
    let public = || {
        b.facts
            .get(target)
            .into_iter()
            .flat_map(|facts| &facts.declarations)
            .filter(|declaration| declaration.exported)
            .flat_map(|declaration| declaration.public_names().iter().cloned())
            .collect::<BTreeSet<_>>()
    };
    let mut modules = BTreeMap::<&str, BTreeSet<String>>::new();
    for (module, facts) in &b.facts {
        if module == target {
            continue;
        }
        for import in facts
            .imports
            .iter()
            .filter(|import| import.target.as_deref() == Some(target))
        {
            let names = modules.entry(module).or_default();
            if import.imported_name == "*" {
                names.extend(public());
            } else if !import.imported_name.is_empty() {
                names.insert(
                    import
                        .local_name
                        .clone()
                        .unwrap_or_else(|| import.imported_name.clone()),
                );
            }
        }
        for reexport in facts
            .reexports
            .iter()
            .filter(|reexport| reexport.target.as_deref() == Some(target))
        {
            modules
                .entry(module)
                .or_default()
                .insert(reexport.local_name.clone());
        }
    }
    for module in b.star_reexporters.get(target).into_iter().flatten() {
        if module != target {
            modules.entry(module).or_default().extend(public());
        }
    }
    let mut out = Vec::new();
    for (importer, facts) in &b.facts {
        if importer == target {
            continue;
        }
        let module = facts.imports.iter().find_map(|import| {
            let module = import.target.as_deref()?;
            (module != importer.as_str()
                && modules
                    .get(module)
                    .is_some_and(|names| names.contains(&import.imported_name)))
            .then_some(module)
        });
        if let Some(module) = module {
            out.push((importer.clone(), module.to_owned()));
        }
    }
    out
}

fn path_analysis(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
) -> Result<(Vec<Value>, Value, Vec<String>, bool), AstGraphError> {
    let file = node_key(
        q.file().ok_or_else(|| {
            AstGraphError::new("invalidGraphQuery", "path requires file and target")
        })?,
        &b.root,
        &b.nodes,
    );
    let target = node_key(
        q.target().ok_or_else(|| {
            AstGraphError::new("invalidGraphQuery", "path requires file and target")
        })?,
        &b.root,
        &b.nodes,
    );
    if !b.nodes.contains_key(&file) || !b.nodes.contains_key(&target) {
        return Err(AstGraphError::new(
            "invalidGraphQuery",
            "file and target must both be in the scanned graph",
        ));
    }
    let mut found = shortest_path(&b.nodes, &file, &target);
    if let Some(edges) = found["edges"].as_array_mut() {
        for edge in edges {
            if let (Some(from), Some(to)) = (edge["from"].as_str(), edge["to"].as_str())
                && let Some(line) = first_import_line(b, from, to)
            {
                edge["importLine"] = json!(line);
            }
        }
    }
    Ok((
        vec![found],
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
    let file_witnesses = CycleWitnesses::new(&b.nodes);
    let runtime_witnesses = CycleWitnesses::new(&runtime);
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
        items.push(json!({"files":files,"edgeKinds":kinds,"runtimeCycle":runtime_count>0,"runtimeCycles":contained,"runtimeCycleCount":runtime_count,"cycleEdges":file_witnesses.witness(&members),"runtimeCycleEdges":runtime_witnesses.witness(&members),"componentId":id,"topologicalLayer":layers.get(&id),"outgoingComponents":outgoing,"confidence":"syntactic"}));
    }
    let rc = items.iter().filter(|x| x["runtimeCycle"] == true).count();
    let ce = c.edges.values().map(BTreeSet::len).sum::<usize>();
    let count = items.len();
    let summary = json!({"cycleCount":count,"runtimeCycleCount":rc,"condensationComponentCount":c.components.len(),"condensationEdgeCount":ce,"topologicalLayerCount":c.layers.len(),"transitiveEdgeCount":trans.len()});
    (items, summary, vec![], false)
}

fn reachability(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    security: &ContentSecurity,
) -> (Vec<Value>, Value, Vec<String>, bool) {
    let (roots, warnings, low) = entrypoints(b, q, security);
    if low && roots.is_empty() {
        return (
            vec![],
            json!({"entrypointsResolvedCount":0,"classifiedCount":0,"unclassifiedCount":b.nodes.len()}),
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
    q: &AstTopologyQuery,
    security: &ContentSecurity,
) -> (Vec<Value>, Value, Vec<String>, bool) {
    let (roots, mut warnings, low_entries) = entrypoints(b, q, security);
    // Hard gate: with no resolvable roots, reachability is undefined and every
    // export would falsely read as dead. Suppress the verdict rather than report
    // the whole tree as dead.
    if roots.is_empty() {
        warnings.push("dead-code verdict suppressed: no entrypoints resolved, so reachability cannot be computed and no export can be proven dead. Pass `entrypoints` explicitly to enable the analysis.".into());
        return (
            vec![],
            json!({"entrypointsResolvedCount":0,"deadClusters":[],"deadClusterCount":0,"deadExportCount":0,"suppressed":true}),
            warnings,
            true,
        );
    }
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
            // An import with known users credits its target only once one
            // of them is live (`declaration_liveness`).
            for i in ff.imports.iter().filter(|i| i.used_in.is_none()) {
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
    let (mut live_ids, path_named) =
        declaration_liveness(b, &live, &rootset, &public, &mut real, &rex, &star);
    let components = scc_unsorted(&b.nodes);
    let mut cluster_by = BTreeMap::new();
    let mut clusters = Vec::new();
    for files in components {
        let report = files
            .into_iter()
            .filter(|f| q.include_tests().unwrap_or(true) || !crate::content::is_test_path(f))
            .collect::<Vec<_>>();
        if report.is_empty() || !report.iter().all(|f| !live.contains(f)) {
            continue;
        }
        let id = clusters.len();
        for f in &report {
            cluster_by.insert(f.clone(), id);
        }
        clusters.push(json!({"id":id,"files":report,"reason":"mutually-referencing cluster with no path from any entrypoint — each file looks locally referenced by the others, but the cluster as a whole is unreachable","edgeKinds":collect_kinds(&b.nodes,&report),"confidence":"syntactic"}));
    }
    let mut rows = Vec::new();
    for (file, ff) in &b.facts {
        if !q.include_tests().unwrap_or(true) && crate::content::is_test_path(file) {
            continue;
        }
        let rust = file.ends_with(".rs");
        let live_ids = live_ids.remove(file.as_str()).unwrap_or_else(|| {
            live_declarations(
                file,
                ff,
                &public,
                &real,
                &rex,
                &star,
                rust && rootset.contains(file),
            )
        });
        // A `mod x;` declaration is module structure: the child file's own
        // liveness is tracked separately.
        for d in ff
            .declarations
            .iter()
            .filter(|d| d.exported && !(rust && d.kind == "module"))
        {
            let mut row = if !live.contains(file) {
                if let Some(id) = cluster_by.get(file) {
                    json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"dead-cluster","clusterId":id})
                } else {
                    json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"unreachable-file"})
                }
            } else if !rootset.contains(file)
                && !star_targets.contains(file)
                && !b.namespace_targets.contains(file)
                && !live_ids.contains(&d.id)
            {
                let via = if d
                    .public_names()
                    .iter()
                    .any(|n| rex.contains_key(&binding(file, n)))
                {
                    "reexport-chain"
                } else if rust && path_named.contains(d.name.as_str()) {
                    "qualified-path-name"
                } else {
                    ff.reference_basis
                };
                json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"unreferenced-export","viaHeuristic":via})
            } else {
                continue;
            };
            if !d.exported_as.is_empty() {
                row["exportedAs"] = json!(d.exported_as);
            }
            rows.push(row);
        }
    }
    let count = rows.len();
    let ccount = clusters.len();
    (
        rows,
        json!({"entrypointsResolved":roots,"entrypointsResolvedCount":roots.len(),"deadClusters":clusters,"deadClusterCount":ccount,"deadExportCount":count}),
        warnings,
        low_entries || b.truncated || b.files_skipped > 0 || !b.diagnostics.is_empty(),
    )
}

fn binding(f: &str, n: &str) -> String {
    format!("{f}::{n}")
}

/// Cross-file liveness per declaration. Two kinds of use credit a binding
/// `(target file, name)` in `real` only from live code (a live declaration,
/// module-level code, or a caller the file does not declare):
/// - an import whose facts name its users (`used_in`); imports without that
///   fact were credited unconditionally by the caller;
/// - a Rust qualified-path call (`crate::portable::sanitize()`), which names
///   an item without a `use`.
///
/// Credits can make new declarations live, so files are re-evaluated in
/// rounds until no binding is added. Returns the final live declaration ids
/// of every live file, and the Rust callee names that live code reaches only
/// by an unresolved path: those stay dead candidates, labelled
/// `qualified-path-name`.
fn declaration_liveness<'b>(
    b: &'b BuiltGraph,
    live: &BTreeSet<String>,
    rootset: &BTreeSet<String>,
    public: &BTreeSet<String>,
    real: &mut BTreeSet<String>,
    rex: &BTreeMap<String, Vec<(String, String)>>,
    star: &BTreeMap<String, Vec<String>>,
) -> (BTreeMap<&'b str, BTreeSet<String>>, BTreeSet<&'b str>) {
    // A credit on a re-exporting file can make the origin's export live.
    let mut origins: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (file, ff) in &b.facts {
        for target in ff.reexports.iter().filter_map(|r| r.target.as_deref()) {
            origins.entry(file).or_default().insert(target);
        }
    }
    for (origin, reexporters) in star {
        if let Some((origin, _)) = b.facts.get_key_value(origin) {
            for reexporter in reexporters {
                origins.entry(reexporter).or_default().insert(origin);
            }
        }
    }
    // Names each file binds (declarations, re-exports, imports), built once
    // per file on first use.
    let mut bound_names: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut declares = |file: &'b str, name: &str| {
        bound_names
            .entry(file)
            .or_insert_with(|| {
                let Some(ff) = b.facts.get(file) else {
                    return BTreeSet::new();
                };
                ff.declarations
                    .iter()
                    .flat_map(|d| d.public_names())
                    .cloned()
                    .chain(ff.reexports.iter().map(|r| r.local_name.clone()))
                    .chain(ff.imports.iter().map(|i| {
                        i.local_name
                            .clone()
                            .unwrap_or_else(|| i.imported_name.clone())
                    }))
                    .collect()
            })
            .contains(name)
    };
    let mut live_ids = BTreeMap::new();
    let mut path_named = BTreeSet::new();
    let mut dirty = b
        .facts
        .keys()
        .filter(|f| live.contains(*f))
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    while !dirty.is_empty() {
        let mut credited = BTreeSet::new();
        for file in std::mem::take(&mut dirty) {
            let Some((file, ff)) = b.facts.get_key_value(file) else {
                continue;
            };
            let ids = live_declarations(file, ff, public, real, rex, star, rootset.contains(file));
            let declared = ff
                .declarations
                .iter()
                .map(|d| d.id.as_str())
                .collect::<BTreeSet<_>>();
            let is_live = |id: &str| ids.contains(id) || !declared.contains(id);
            for import in &ff.imports {
                let (Some(target), Some(users)) = (&import.target, &import.used_in) else {
                    continue;
                };
                if users
                    .iter()
                    .any(|user| user == IMPORT_USE_MODULE || is_live(user))
                    && real.insert(binding(target, &import.imported_name))
                {
                    credited.insert(target.as_str());
                }
            }
            let calls = if file.ends_with(".rs") {
                ff.calls.as_slice()
            } else {
                &[]
            };
            for call in calls {
                let Some((_, name)) = call.callee.rsplit_once("::") else {
                    continue;
                };
                if !call.caller_id.as_deref().is_none_or(is_live) || name.is_empty() {
                    continue;
                }
                match &call.target {
                    Some(target) => {
                        if !declares(target, name) {
                            path_named.insert(name);
                        }
                        if real.insert(binding(target, name)) {
                            credited.insert(target.as_str());
                        }
                    }
                    None => {
                        path_named.insert(name);
                    }
                }
            }
            live_ids.insert(file.as_str(), ids);
        }
        let mut pending = credited.into_iter().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(file) = pending.pop() {
            if !seen.insert(file) {
                continue;
            }
            if live.contains(file) {
                dirty.insert(file);
            }
            pending.extend(origins.get(file).into_iter().flatten().copied());
        }
    }
    (live_ids, path_named)
}
/// Declaration ids of `file` that are live: reachable over syntactic call and
/// containment edges from the file's roots. Roots are exported declarations
/// consumed through an import or re-export chain, declarations that escape as
/// a value (syntax-aware references other than the declaration, export clauses
/// and call targets), and callees of module-level code. Identity is the
/// declaration id, so two declarations with one display name (`run` and a
/// `Calls.run` method) keep separate liveness.
fn live_declarations(
    file: &str,
    ff: &FileFacts,
    public: &BTreeSet<String>,
    real: &BTreeSet<String>,
    rex: &BTreeMap<String, Vec<(String, String)>>,
    star: &BTreeMap<String, Vec<String>>,
    entry: bool,
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
            // `export *` never re-exports `default`.
            if n != "default" {
                pending.extend(
                    star.get(&f)
                        .into_iter()
                        .flatten()
                        .map(|x| (x.clone(), n.clone())),
                )
            }
        }
        false
    }
    /// Last path segment of a callee (`this.run` → `run`, `Self::new` → `new`).
    fn callee_name(callee: &str) -> &str {
        callee
            .rsplit(['.', ':'])
            .next()
            .filter(|n| !n.is_empty())
            .unwrap_or(callee)
    }
    let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for d in &ff.declarations {
        by_name.entry(&d.name).or_default().push(&d.id);
        if let Some(parent) = &d.parent {
            children.entry(parent).or_default().push(&d.id);
        }
    }
    let ids = ff
        .declarations
        .iter()
        .map(|d| d.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut live = BTreeSet::new();
    let mut pending = VecDeque::new();
    fn mark(id: &str, live: &mut BTreeSet<String>, pending: &mut VecDeque<String>) {
        if live.insert(id.to_owned()) {
            pending.push_back(id.to_owned());
        }
    }
    for c in &ff.calls {
        let targets = by_name
            .get(callee_name(&c.callee))
            .map(Vec::as_slice)
            .unwrap_or_default();
        match c.caller_id.as_deref().filter(|id| ids.contains(id)) {
            Some(caller) => edges.entry(caller).or_default().extend(targets),
            // Module-level code runs when the (live) module loads.
            None => {
                for t in targets {
                    mark(t, &mut live, &mut pending)
                }
            }
        }
    }
    for d in &ff.declarations {
        let is_consumed = d.exported
            && d.public_names()
                .iter()
                .any(|n| consumed(file, n, public, real, rex, star));
        // Counts exclude the declaration, export clauses and call targets, so
        // any remaining reference is a value escape. A missing count means the
        // producer could not count this declaration: treat it as escaping
        // rather than risk a false dead verdict.
        let escapes = ff
            .reference_counts
            .get(&d.id)
            .is_none_or(|count| *count > 0);
        // A Rust root's top-level `fn main` is where the program starts.
        let is_entry = entry && d.name == "main" && d.parent.is_none();
        if is_consumed || escapes || is_entry {
            mark(&d.id, &mut live, &mut pending)
        }
    }
    while let Some(id) = pending.pop_front() {
        for t in edges
            .get(id.as_str())
            .into_iter()
            .flatten()
            .chain(children.get(id.as_str()).into_iter().flatten())
        {
            mark(t, &mut live, &mut pending)
        }
    }
    live
}

fn entrypoints(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    security: &ContentSecurity,
) -> (Vec<String>, Vec<String>, bool) {
    let mut roots = Vec::new();
    let mut seen_roots = BTreeSet::new();
    let mut warnings = Vec::new();
    let mut low = false;
    if let Some(explicit) = q.entrypoints().filter(|x| !x.is_empty()) {
        for raw in explicit {
            let p = node_key(raw, &b.root, &b.nodes);
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
        // Package.json is JS-only. Infer roots for the other ecosystems so a
        // Rust/Go tree does not read as entirely dead. Inferrers are additive: a
        // mixed repository can contribute roots from several ecosystems.
        infer_rust_roots(b, security, &mut roots, &mut seen_roots);
        infer_go_roots(b, &mut roots, &mut seen_roots);
        if roots.is_empty() {
            warnings.push("no entrypoints resolved for the detected languages — expected package.json main/bin/exports, a Cargo.toml target or src/main.rs|src/lib.rs|src/bin/*.rs, or a Go `func main`. Pass `entrypoints` explicitly; without a root every export reads as unreachable, so the dead-code verdict is suppressed.".into());
            low = true
        }
    }
    if q.include_tests().unwrap_or(true) {
        for file in b
            .nodes
            .keys()
            .filter(|x| crate::content::is_test_path(x))
            .cloned()
        {
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

/// Infer Rust crate entrypoints: `[[bin]]`/`[lib]` (and other) `path = "…"`
/// targets declared in a root `Cargo.toml`, plus the conventional
/// `src/main.rs`, `src/lib.rs`, and `src/bin/*.rs` targets (also matched for
/// workspace members via their path suffix). Only node keys that actually exist
/// in the scanned graph are added.
fn infer_rust_roots(
    b: &BuiltGraph,
    security: &ContentSecurity,
    roots: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
) {
    let has_rust = b.nodes.keys().any(|k| k.ends_with(".rs"));
    if !has_rust {
        return;
    }
    let cargo = b.root.join("Cargo.toml");
    if let Ok(bytes) = fs::read(&cargo)
        && let Ok(safe) = security.validate_text_bytes(&bytes, Some(&cargo), 1_000_000)
    {
        for path in cargo_target_paths(&safe.content) {
            let key = normalize(path.trim_start_matches("./"));
            if b.nodes.contains_key(&key) {
                push_unique(roots, seen, key);
            }
        }
    }
    for key in b.nodes.keys() {
        if is_rust_conventional_root(key) {
            push_unique(roots, seen, key.clone());
        }
    }
}

/// A node key that Cargo treats as a default target: crate roots (`main.rs`,
/// `lib.rs`, at the tree root or as a `src/` child, including workspace members)
/// and binaries under a `src/bin/` directory.
fn is_rust_conventional_root(key: &str) -> bool {
    key == "main.rs"
        || key == "lib.rs"
        || key == "src/main.rs"
        || key == "src/lib.rs"
        || key.ends_with("/main.rs")
        || key.ends_with("/lib.rs")
        || (key.ends_with(".rs") && (key.starts_with("src/bin/") || key.contains("/src/bin/")))
}

/// Extract quoted values of `path = "…"` keys from a Cargo.toml. Kept
/// intentionally lenient (no TOML dependency): callers only add targets that
/// resolve to a real scanned node, so a stray candidate is harmless.
fn cargo_target_paths(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("path") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        if let Some(inner) = rest
            .strip_prefix('"')
            .and_then(|r| r.split('"').next())
            .filter(|s| !s.is_empty())
        {
            out.push(inner.to_owned());
        }
    }
    out
}

/// Infer Go entrypoints: any `.go` file that declares a `func main` — the
/// signature of a `package main` executable. Test files are excluded here (they
/// are added separately as roots when tests are included).
fn infer_go_roots(b: &BuiltGraph, roots: &mut Vec<String>, seen: &mut BTreeSet<String>) {
    for (file, facts) in &b.facts {
        if !file.ends_with(".go") {
            continue;
        }
        let has_main = facts
            .declarations
            .iter()
            .any(|d| d.name == "main" && (d.kind == "function" || d.kind == "func"));
        if has_main {
            push_unique(roots, seen, file.clone());
        }
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

/// Compare the import topology of a baseline root against the head `path` and
/// report typed structural drift. Builds two independent snapshots and diffs
/// them through the shared engine so results stay AST-only and deterministic.
pub(crate) fn drift(
    q: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    let head = super::graph::build_graph(q, paths, security, cancel)?;
    let baseline_root = q
        .baseline()
        .map(str::to_owned)
        .ok_or_else(|| AstGraphError::new("invalidGraphQuery", "drift requires baseline"))?;
    let mut base_query = q.clone();
    base_query.set_path(baseline_root);
    let mut base = super::graph::build_graph(&base_query, paths, security, cancel)?;
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

    let snapshot = digest(&items);
    if q.diagnostic_snapshot()
        .is_some_and(|expected| expected != snapshot)
    {
        base_map.insert("results".into(), json!([]));
        insert_snapshot_changed(&mut base_map);
        base_map.insert(
            "next".into(),
            json!({"restartDiagnostics": restart_continuation(q)}),
        );
        return Ok(Value::Object(base_map));
    }
    let (page, pagination, limit_truncated, total) = paginate(items, q);
    let has_more = pagination["hasMore"] == json!(true);
    let mut warnings = Vec::new();
    if !diff.comparable {
        base_map.insert("confidence".into(), json!("low"));
        warnings.push("graphs are not comparable — see summary.incompatibilities".to_owned());
    }
    warnings.extend(out_of_range_warning(&pagination));
    base_map.insert("results".into(), Value::Array(page));
    base_map.insert("pagination".into(), pagination);
    base_map.insert("summary".into(), summary);
    if !warnings.is_empty() {
        base_map.insert("warnings".into(), json!(warnings));
    }

    let mut reasons: Vec<&str> = Vec::new();
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
    if has_more && q.page() < 1000 {
        let mut next = continuation(
            q,
            Some(q.page() + 1),
            None,
            "Continue topology drift results.",
        );
        next["query"]["diagnosticSnapshot"] = json!(snapshot);
        base_map["pagination"]["resultId"] = json!(snapshot);
        base_map.insert("next".into(), json!({ "nextPage": next }));
    }
    Ok(Value::Object(base_map))
}

fn paginate(items: Vec<Value>, q: &AstTopologyQuery) -> (Vec<Value>, Value, bool, usize) {
    let total = items.len();
    let limited = if let Some(l) = q.limit() {
        items.into_iter().take(l as usize).collect()
    } else {
        items
    };
    let truncated = limited.len() < total;
    let size = q.page_size().clamp(1, super::topology_max("pageSize")) as usize;
    let pages = usize::max(1, limited.len().div_ceil(size));
    // A page past the end is empty, terminal and flagged — never clamped to
    // the last page, which would repeat rows the caller already has.
    let current = q.page().max(1) as usize;
    let out_of_range = current > pages;
    let start = (current - 1).saturating_mul(size);
    let mut pagination = json!({
        "currentPage": current,
        "totalPages": pages,
        "entriesPerPage": size,
        "totalEntries": limited.len(),
        "hasMore": !out_of_range && current < pages
    });
    if out_of_range {
        pagination["outOfRange"] = json!(true);
    }
    (
        limited.into_iter().skip(start).take(size).collect(),
        pagination,
        truncated,
        total,
    )
}

/// The warning for a result page past the end, naming the valid range.
fn out_of_range_warning(pagination: &Value) -> Option<String> {
    (pagination["outOfRange"] == true).then(|| {
        let pages = pagination["totalPages"].as_u64().unwrap_or(1);
        format!(
            "page:{} is out of range (only {pages} page(s), {} result(s)) — returned 0 results. Use page:1..{pages}.",
            pagination["currentPage"],
            pagination["totalEntries"]
        )
    })
}

/// Stable identity of a JSON sequence.
fn digest<T: serde::Serialize>(value: &T) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).unwrap_or_default(),
    ))
}

/// Mark a replayed page whose graph snapshot no longer matches.
fn insert_snapshot_changed(base: &mut Map<String, Value>) {
    base.insert("status".into(), json!("error"));
    base.insert("errorCode".into(), json!("graphSnapshotChanged"));
    base.insert(
        "error".into(),
        json!(
            "Graph results or diagnostics changed between pages. Restart before combining pages."
        ),
    );
}

/// Restart both result and diagnostic pagination from the current graph.
fn restart_continuation(q: &AstTopologyQuery) -> Value {
    let mut query = clean_query(q);
    query["page"] = json!(1);
    if let Some(query) = query.as_object_mut() {
        query.remove("diagnosticSnapshot");
        query.remove("diagnosticPage");
    }
    json!({"tool":ToolId::AstTopology.as_str(),"query":query,"why":"Restart pagination from the current graph snapshot.","confidence":"exact"})
}

/// Coverage outcome of one response: whether the caller's diagnostic snapshot
/// is stale, and the snapshot id of diagnostic rows withheld by the
/// counts-only default (rows stay reachable through `next.nextDiagnostics`).
struct CoverageState {
    changed: bool,
    withheld: Option<String>,
}

/// Attach coverage and diagnostics. The snapshot id binds both the diagnostic
/// list and the full result list, so a replayed result or diagnostic page from
/// a changed graph is rejected instead of silently mixing graph versions.
/// Diagnostic rows are returned only for an explicit `diagnosticPage`; the
/// default carries `diagnosticCounts` and gap reasons, and rows with the same
/// code and message are grouped into one row listing every `path[:line]`.
fn add_coverage(
    base: &mut Map<String, Value>,
    b: &mut BuiltGraph,
    q: &AstTopologyQuery,
    results_digest: &str,
) -> CoverageState {
    b.diagnostics.sort();
    b.diagnostics.dedup();
    let tuples = b
        .diagnostics
        .iter()
        .map(|d| json!([d.file, d.line, d.code, d.message]))
        .collect::<Vec<_>>();
    let id = digest(&json!([tuples, results_digest]));
    let mut counts = BTreeMap::<String, u32>::new();
    for d in &b.diagnostics {
        *counts.entry(d.code.clone()).or_default() += 1
    }
    let languages=b.languages.iter().map(|(language,files,linking)|json!({"language":language,"files":files,"linking":linking})).collect::<Vec<_>>();
    let mut imports = json!({"resolved":b.imports[0],"external":b.imports[1],"unresolvedInternal":b.imports[2],"unsupported":b.imports[3]});
    if b.imports[4] > 0 {
        imports["nonCode"] = json!(b.imports[4]);
    }
    let mut coverage = json!({"basis":"syntactic","referenceBasis":"lexical-occurrence","languages":languages,"imports":imports});
    if !counts.is_empty() {
        coverage["diagnosticCounts"] = json!(counts);
    }
    if q.diagnostic_snapshot().as_ref().is_some_and(|x| x != &id) {
        insert_snapshot_changed(base);
        base.insert("results".into(), json!([]));
        base.insert("coverage".into(), coverage);
        return CoverageState {
            changed: true,
            withheld: None,
        };
    }
    if base["pagination"]["hasMore"] == true {
        base["pagination"]["resultId"] = json!(id);
    }
    if !q.diagnostic_rows_requested() {
        base.insert("coverage".into(), coverage);
        return CoverageState {
            changed: false,
            withheld: (!b.diagnostics.is_empty()).then_some(id),
        };
    }
    let groups = group_diagnostics(&b.diagnostics);
    let size = q
        .diagnostic_page_size()
        .clamp(1, super::topology_max("diagnosticPageSize")) as usize;
    let pages = usize::max(1, groups.len().div_ceil(size));
    let current = (q.diagnostic_page().max(1) as usize).min(pages);
    let more = current < pages;
    let ds = groups
        .into_iter()
        .skip((current - 1) * size)
        .take(size)
        .collect::<Vec<_>>();
    let total = b
        .diagnostics
        .iter()
        .map(|d| (&d.code, &d.message))
        .collect::<BTreeSet<_>>()
        .len();
    let mut diagnostics_pagination = json!({"currentPage":current,"totalPages":pages,"entriesPerPage":size,"totalEntries":total,"hasMore":more,"resultId":id});
    if q.diagnostic_page() as usize > pages {
        diagnostics_pagination["outOfRange"] = json!(true);
        let warning = format!(
            "diagnosticPage:{} is out of range; returned diagnostic page {}.",
            q.diagnostic_page(),
            current
        );
        match base.get_mut("warnings") {
            Some(Value::Array(warnings)) => warnings.push(json!(warning)),
            _ => {
                base.insert("warnings".into(), json!([warning]));
            }
        }
    }
    if !ds.is_empty() {
        coverage["diagnostics"] = json!(ds);
    }
    // One diagnostic page needs no pagination envelope; emit it only when a
    // continuation or an out-of-range correction depends on it.
    if pages > 1 || diagnostics_pagination["outOfRange"] == true {
        coverage["diagnosticsPagination"] = diagnostics_pagination;
    }
    base.insert("coverage".into(), coverage);
    CoverageState {
        changed: false,
        withheld: None,
    }
}

/// One row per distinct code+message, in first-file order. A single
/// occurrence keeps `file`/`line`; repeated ones list `files` as `path[:line]`.
fn group_diagnostics(diagnostics: &[Diagnostic]) -> Vec<Value> {
    let mut order = Vec::<(&str, &str)>::new();
    let mut members = BTreeMap::<(&str, &str), Vec<&Diagnostic>>::new();
    for d in diagnostics {
        let key = (d.code.as_str(), d.message.as_str());
        let list = members.entry(key).or_default();
        if list.is_empty() {
            order.push(key);
        }
        list.push(d);
    }
    order
        .into_iter()
        .map(|key| match members[&key].as_slice() {
            [single] => json!(single),
            many => json!({
                "code": key.0,
                "message": key.1,
                "files": many
                    .iter()
                    .map(|d| match d.line {
                        Some(line) => format!("{}:{line}", d.file),
                        None => d.file.clone(),
                    })
                    .collect::<Vec<_>>()
            }),
        })
        .collect()
}
fn add_next(
    base: &mut Map<String, Value>,
    q: &AstTopologyQuery,
    root: &Path,
    limit_truncated: bool,
    scan_truncated: bool,
    withheld_diagnostics: Option<&str>,
) {
    let mut next = Map::new();
    if base["pagination"]["hasMore"] == true && q.page() < 1000 {
        let why = match q.analysis() {
            GraphAnalysis::Dependencies => "Continue dependencies.",
            GraphAnalysis::Dependents => "Continue dependents.",
            GraphAnalysis::Path => "Continue path results.",
            GraphAnalysis::Cycles => "Continue cycle components.",
            GraphAnalysis::Reachability => "Continue reachability classifications.",
            GraphAnalysis::DeadCode => "Continue dead-code candidates.",
            GraphAnalysis::Drift => "Continue topology drift results.",
        };
        let mut value = continuation(q, Some(q.page() + 1), None, why);
        // Bind the next result page to this graph snapshot.
        if let Some(snapshot) = base["pagination"].get("resultId") {
            value["query"]["diagnosticSnapshot"] = snapshot.clone();
        }
        next.insert("nextPage".into(), value);
    }
    if let Some(snapshot) = withheld_diagnostics {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(1);
        value["diagnosticPageSize"] = json!(q.diagnostic_page_size());
        value["diagnosticSnapshot"] = json!(snapshot);
        next.insert(
            "nextDiagnostics".into(),
            json!({"tool":ToolId::AstTopology.as_str(),"query":value,"why":"Coverage diagnostic rows behind diagnosticCounts.","confidence":"exact"}),
        );
    }
    if base["coverage"]["diagnosticsPagination"]["hasMore"] == true && q.diagnostic_page() < 1000 {
        let mut value = clean_query(q);
        value["diagnosticPage"] = json!(q.diagnostic_page() + 1);
        value["diagnosticPageSize"] = json!(q.diagnostic_page_size());
        value["diagnosticSnapshot"] = base["coverage"]["diagnosticsPagination"]["resultId"].clone();
        next.insert(
            "nextDiagnostics".into(),
            json!({"tool":ToolId::AstTopology.as_str(),"query":value,"why":"Continue coverage diagnostics from the same diagnostic snapshot.","confidence":"exact"}),
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
            json!({"tool":ToolId::AstTopology.as_str(),"query":value,"why":"Restart diagnostic pagination from the current diagnostic snapshot.","confidence":"exact"}),
        );
    }
    let max_files = super::topology_max("maxFiles");
    if scan_truncated && q.max_files().unwrap_or(20_000) < max_files {
        let cur = q.max_files().unwrap_or(20_000);
        next.insert(
            "expandScan".into(),
            continuation(
                q,
                Some(1),
                Some((cur * 2).max(cur + 1).min(max_files)),
                "Re-run with a larger file-scan bound because this graph is partial.",
            ),
        );
    }
    let max_limit = super::topology_max("limit");
    if let Some(limit) = q.limit().filter(|limit| *limit < max_limit)
        && limit_truncated
    {
        let mut value = clean_query(q);
        value["limit"] = json!((limit * 2).max(limit + 1).min(max_limit));
        value["page"] = json!(1);
        restart_diagnostic_rows(&mut value, q);
        next.insert("expandLimit".into(),json!({"tool":ToolId::AstTopology.as_str(),"query":value,"why":"Re-run with a larger result limit because additional graph results exist.","confidence":"exact"}));
    }
    if q.analysis() == GraphAnalysis::DeadCode
        && let Some(c) = base["results"].as_array().and_then(|x| x.first())
        && let (Some(file), Some(name), Some(line)) =
            (c["file"].as_str(), c["name"].as_str(), c["line"].as_u64())
    {
        // The uri anchors on the canonical graph root (inferred when `path` is
        // omitted). The output contract validates this advisory continuation
        // against the lspSearch anchored-query schema, whose serialization
        // requires every defaulted field; emit exactly those contract fields.
        let uri = root.join(file).to_string_lossy().into_owned();
        next.insert("verifyReferences".into(),json!({"tool":ToolId::LspSearch.as_str(),"query":{"operation":"references","uri":uri,"symbolName":name,"lineHint":line,"includeDeclaration":false,"groupByFile":true,"orderHint":0,"page":1,"debug":false},"why":format!("Verify candidate \"{name}\" before deletion; repeat for each result, prioritizing viaHeuristic:\"reexport-chain\"."),"confidence":"high"}));
    }
    if !next.is_empty() {
        base.insert("next".into(), Value::Object(next));
    }
}
fn clean_query(q: &AstTopologyQuery) -> Value {
    let mut v = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(m) = v.as_object_mut() {
        m.retain(|_, x| !x.is_null());
        if m.get("diagnosticPage") == Some(&json!(1)) {
            m.remove("diagnosticPage");
        }
    }
    v
}
/// A re-run over a new graph restarts diagnostic paging: rows stay requested
/// only when this query requested them, and the old snapshot is dropped.
fn restart_diagnostic_rows(query: &mut Value, q: &AstTopologyQuery) {
    if let Some(fields) = query.as_object_mut() {
        fields.remove("diagnosticSnapshot");
        if q.diagnostic_rows_requested() {
            fields.insert("diagnosticPage".into(), json!(1));
        } else {
            fields.remove("diagnosticPage");
        }
    }
}
fn continuation(q: &AstTopologyQuery, page: Option<u32>, max: Option<u32>, why: &str) -> Value {
    let mut v = clean_query(q);
    if let Some(x) = page {
        v["page"] = json!(x)
    }
    if let Some(x) = max {
        v["maxFiles"] = json!(x);
        restart_diagnostic_rows(&mut v, q);
    }
    json!({"tool":ToolId::AstTopology.as_str(),"query":v,"why":why,"confidence":"exact"})
}
/// `file` resolves relative to the scanned `path`; name that rule and, when a
/// scanned file shares the requested suffix, the spelling that would match.
fn missing_file_message(file: &str, nodes: &BTreeMap<String, Node>) -> String {
    let mut message = format!(
        "file is not in the scanned graph: {file}. `file` is relative to `path` \
         (or absolute under it), and must be a scanned source file"
    );
    if let Some(candidate) = suffix_candidate(file, nodes) {
        message.push_str(&format!("; did you mean `{candidate}`?"));
    }
    message
}

fn suffix_candidate<'a>(file: &str, nodes: &'a BTreeMap<String, Node>) -> Option<&'a String> {
    let suffix = format!("/{file}");
    nodes
        .keys()
        .find(|key| key.ends_with(&suffix) || file.ends_with(&format!("/{key}")))
}

/// The missing-file error, with an executable `next.retry` on the suffix
/// candidate when one exists.
fn missing_file_error(
    q: &AstTopologyQuery,
    file: &str,
    nodes: &BTreeMap<String, Node>,
) -> AstGraphError {
    let mut error = AstGraphError::new("invalidGraphQuery", missing_file_message(file, nodes));
    if let Some(candidate) = suffix_candidate(file, nodes) {
        let mut query = clean_query(q);
        query["file"] = json!(candidate);
        error.next = Some(Box::new(json!({"retry": {
            "tool": ToolId::AstTopology.as_str(),
            "query": query,
            "why": "Retry with the scanned file that shares this path suffix.",
            "confidence": "medium"
        }})));
    }
    error
}

/// A graph key for `raw`: relative to the scanned root, or absolute under it.
/// A workspace-relative spelling of a file under the root (leading components
/// that name the root's own tail) resolves to the same key.
fn node_key(raw: &str, root: &Path, nodes: &BTreeMap<String, Node>) -> String {
    let key = graph_file(raw, root);
    if nodes.contains_key(&key) || Path::new(raw).is_absolute() {
        return key;
    }
    let parts = key.split('/').collect::<Vec<_>>();
    (1..parts.len())
        .find_map(|split| {
            let (prefix, rest) = parts.split_at(split);
            let rest = rest.join("/");
            (root.ends_with(prefix.join("/")) && nodes.contains_key(&rest)).then_some(rest)
        })
        .unwrap_or(key)
}

fn graph_file(f: &str, root: &Path) -> String {
    let p = Path::new(f);
    if p.is_absolute() {
        let canonical = fs::canonicalize(p).ok();
        canonical
            .as_deref()
            .unwrap_or(p)
            .strip_prefix(root)
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
/// The first import (or, failing one, re-export) statement in `importer`
/// that links `target`.
fn first_import_line(b: &BuiltGraph, importer: &str, target: &str) -> Option<u32> {
    let facts = b.facts.get(importer)?;
    facts
        .imports
        .iter()
        .find(|i| i.target.as_deref() == Some(target))
        .map(|i| i.line)
        .or_else(|| {
            facts
                .reexport_lines
                .iter()
                .filter(|(linked, _)| linked == target)
                .map(|(_, line)| *line)
                .min()
        })
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
        // This branch is only reached with a frame on the stack.
        #[allow(clippy::expect_used)]
        let node = frames.pop().expect("frame exists").node;
        postorder.push(node);
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

    #[cfg(unix)]
    #[test]
    fn graph_file_resolves_absolute_paths_through_symlinked_roots() {
        let temp = tempfile::tempdir().expect("temporary workspace");
        let root = temp.path().join("actual");
        fs::create_dir(&root).expect("source directory");
        fs::write(root.join("entry.ts"), "export const entry = 1;").expect("source file");
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).expect("workspace alias");
        let canonical_root = fs::canonicalize(&root).expect("canonical root");

        assert_eq!(
            graph_file(&alias.join("entry.ts").to_string_lossy(), &canonical_root),
            "entry.ts"
        );
        assert_eq!(
            graph_file(&root.join("entry.ts").to_string_lossy(), &canonical_root),
            "entry.ts"
        );
        assert_eq!(graph_file("entry.ts", &canonical_root), "entry.ts");
        assert_eq!(
            graph_file(
                &canonical_root.join("missing.ts").to_string_lossy(),
                &canonical_root
            ),
            "missing.ts"
        );
    }

    #[test]
    fn diagnostic_continuation_stays_on_ast_topology() {
        let query: AstTopologyQuery = serde_json::from_value(json!({
            "goal": "test", "reasoning":"test",
            "analysis":"dependencies",
            "path":".",
            "file":"src/index.ts",
            "diagnosticPageSize":2
        }))
        .expect("graph query");
        let mut result = json!({
            "pagination":{"hasMore":false},
            "coverage":{"diagnosticsPagination":{
                "hasMore":true,
                "outOfRange":false,
                "resultId":"diagnostic-snapshot"
            }},
            "results":[]
        })
        .as_object()
        .cloned()
        .expect("result object");

        add_next(&mut result, &query, Path::new("/repo"), false, false, None);

        assert_eq!(result["next"]["nextDiagnostics"]["tool"], "astTopology");
        assert!(
            result["next"]["nextDiagnostics"]["query"]
                .get("operation")
                .is_none()
        );
        assert_eq!(
            result["next"]["nextDiagnostics"]["query"]["diagnosticSnapshot"],
            "diagnostic-snapshot"
        );

        // Counts-only default: withheld rows are offered from page 1 of the
        // same snapshot.
        let mut counts_only = json!({"pagination":{"hasMore":false},"coverage":{},"results":[]})
            .as_object()
            .cloned()
            .expect("result object");
        add_next(
            &mut counts_only,
            &query,
            Path::new("/repo"),
            false,
            false,
            Some("withheld-snapshot"),
        );
        let offered = &counts_only["next"]["nextDiagnostics"]["query"];
        assert_eq!(offered["diagnosticPage"], 1);
        assert_eq!(offered["diagnosticPageSize"], 2);
        assert_eq!(offered["diagnosticSnapshot"], "withheld-snapshot");
    }

    #[test]
    fn identical_diagnostics_group_into_one_row_with_their_files() {
        let diagnostic = |file: &str, line: Option<u32>, message: &str| Diagnostic {
            file: file.into(),
            line,
            code: "unsupported-linking".into(),
            message: message.into(),
        };
        let rows = group_diagnostics(&[
            diagnostic("a.rs", Some(3), "macro"),
            diagnostic("b.rs", None, "macro"),
            diagnostic("c.rs", Some(9), "other"),
        ]);
        assert_eq!(
            rows,
            vec![
                json!({"code":"unsupported-linking","message":"macro","files":["a.rs:3","b.rs"]}),
                json!({"file":"c.rs","line":9,"code":"unsupported-linking","message":"other"}),
            ]
        );
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

    #[test]
    fn workspace_relative_files_under_path_resolve_and_misses_get_a_repair() {
        let root = Path::new("/ws/packages/pkg");
        let nodes = BTreeMap::from([("src/index.ts".into(), node(&[]))]);
        for spelling in [
            "src/index.ts",
            "pkg/src/index.ts",
            "packages/pkg/src/index.ts",
        ] {
            assert_eq!(
                node_key(spelling, root, &nodes),
                "src/index.ts",
                "{spelling}"
            );
        }
        assert_eq!(
            node_key("other/src/index.ts", root, &nodes),
            "other/src/index.ts"
        );

        let query: AstTopologyQuery = serde_json::from_value(json!({
            "goal": "test", "reasoning":"test",
            "analysis":"dependencies",
            "path":"packages/pkg",
            "file":"lib/src/index.ts"
        }))
        .expect("graph query");
        let error = missing_file_error(&query, "lib/src/index.ts", &nodes);
        let next = error.next.expect("repair continuation");
        assert_eq!(next["retry"]["tool"], "astTopology", "{next}");
        assert_eq!(next["retry"]["query"]["file"], "src/index.ts", "{next}");
        assert_eq!(next["retry"]["query"]["path"], "packages/pkg", "{next}");
    }

    #[test]
    fn missing_file_names_the_path_relative_rule_and_a_suffix_match() {
        let graph = BTreeMap::from([("src/index.ts".into(), node(&[]))]);
        let message = missing_file_message("index.ts", &graph);
        assert!(message.contains("relative to `path`"), "{message}");
        assert!(message.contains("did you mean `src/index.ts`"), "{message}");
        let message = missing_file_message("pkg/src/index.ts", &graph);
        assert!(message.contains("did you mean `src/index.ts`"), "{message}");
        assert!(!missing_file_message("other.ts", &graph).contains("did you mean"));
    }
}
