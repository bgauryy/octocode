use super::{algorithms::*, graph::normalize, liveness::*, page::*, types::*};
use crate::tools::id::ToolId;
use crate::tools::id::query_limits::ast_topology::{DIAGNOSTIC_PAGE_MAXIMUM, PAGE_MAXIMUM};
use crate::tools::result::Continuation;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub(crate) fn analyze(
    b: &mut BuiltGraph,
    q: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> AstGraphResult {
    let read = |path: &std::path::Path| super::aliases::read_config_text(paths, security, path);
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
    let mut base = Map::new();
    base.insert("operation".into(), json!("topology"));
    base.insert("path".into(), json!(b.display_path));
    base.insert("filesScanned".into(), json!(b.facts.len()));
    let (items, mut summary, extra_warnings, mut low) = match q.analysis() {
        GraphAnalysis::Dependencies | GraphAnalysis::Dependents => traversal(b, q)?,
        GraphAnalysis::Path => path_analysis(b, q)?,
        GraphAnalysis::Cycles => cycles(b),
        GraphAnalysis::Reachability => reachability(b, q, &read),
        GraphAnalysis::DeadCode => dead_code(b, q, &read),
        GraphAnalysis::Drift => {
            return Err(AstGraphError::new(
                "invalidGraphQuery",
                "drift is dispatched before analyze",
            ));
        }
    };
    warnings.extend(extra_warnings);
    drop_repeated_summary_lists(&mut summary, q);
    let gaps = CoverageGaps::of(b);
    // Import-resolution health drives whether an edge-derived answer can be
    // trusted, so an incomplete import graph never presents as a confident
    // zero (e.g. `cycleCount:0` when no `crate::` import could be resolved).
    // It only ever escalates to low.
    if gaps.unresolved || gaps.unsupported {
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
    let results_digest = crate::digest::json_sha256(&items);
    let (page, pagination) = paginate(items, q);
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
    let coverage_state = add_coverage(&mut base, b, q, &results_digest);
    let result_page_limit = mark_scope_cuts(&mut base, b, q, coverage_state.changed);
    // A diagnostic page reached from a lead (it carries the snapshot) shows
    // the diagnostics only: its results were delivered by the offering page.
    let diagnostics_only = q.diagnostic_rows_requested() && q.diagnostic_snapshot().is_some();
    if diagnostics_only && !coverage_state.changed {
        keep_diagnostics_only(&mut base);
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
            b.truncated,
            coverage_state.withheld.as_deref(),
        );
        if let Some(read) = read_lead(b, q, &base) {
            let mut next = Map::new();
            next.insert("read".into(), read);
            if let Some(Value::Object(rest)) = base.shift_remove("next") {
                next.extend(rest);
            }
            base.insert("next".into(), Value::Object(next));
        }
    }
    let result_state = if diagnostics_only {
        "complete"
    } else if result_page_limit {
        "truncated"
    } else if base["pagination"]["hasMore"] == true {
        "pageable"
    } else if b.truncated {
        "truncated"
    } else {
        "complete"
    };
    add_completeness(&mut base, b, result_state, &gaps.reasons());
    base.insert("operation".into(), json!(q.analysis().as_str()));
    Ok(Value::Object(base))
}

/// The resolved root and dynamic-only lists are page-invariant: emit them
/// with the first result page only; later pages keep the counts. Explicit
/// entrypoints that all resolved are an echo of the request: the count
/// stays, the list is kept only when it adds information (inferred roots,
/// or some explicit root did not resolve).
pub(super) fn drop_repeated_summary_lists(summary: &mut Value, q: &AstTopologyQuery) {
    let Some(obj) = summary.as_object_mut() else {
        return;
    };
    if q.page() > 1 {
        obj.remove("entrypointsResolved");
        obj.remove("dynamicOnlyFiles");
    }
    // The traversal's resolved source echoes the query's when spelled alike.
    if q.source().is_some() && obj.get("source").and_then(Value::as_str) == q.source() {
        obj.remove("source");
    }

    if let Some(explicit) = q.entrypoints().filter(|x| !x.is_empty())
        && obj.get("entrypointsResolvedCount").and_then(Value::as_u64)
            == Some(explicit.len() as u64)
    {
        obj.remove("entrypointsResolved");
    }
}

/// Coverage gaps of the linked graph. They are not truncation: every page
/// is still reachable, so they surface only as `completeness.graph` plus
/// `confidence`; each gap's count is in `coverage`.
pub(super) struct CoverageGaps {
    parse: bool,
    unresolved: bool,
    unsupported: bool,
}

impl CoverageGaps {
    fn of(b: &BuiltGraph) -> Self {
        let has = |code: &str| b.diagnostics.iter().any(|d| d.code == code);
        Self {
            parse: has("parse-recovery"),
            unresolved: has("unresolved-internal"),
            unsupported: has("unsupported-linking"),
        }
    }
    fn reasons(&self) -> Vec<&'static str> {
        [
            (self.parse, "parseRecovery"),
            (self.unresolved, "unresolvedImports"),
            (self.unsupported, "unsupportedLinking"),
        ]
        .into_iter()
        .filter_map(|(present, reason)| present.then_some(reason))
        .collect()
    }
}

/// Real scope cuts (result page ceiling, file-scan bound, skipped files):
/// `isPartial` with `partialReasons`, and `terminalLimit` when no
/// continuation can reach the rest. Returns whether the result page
/// ceiling was hit.
pub(super) fn mark_scope_cuts(
    base: &mut Map<String, Value>,
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    snapshot_changed: bool,
) -> bool {
    let mut reasons = Vec::<String>::new();
    if b.truncated {
        reasons.push("maxFiles".into());
    }
    if b.files_skipped > 0 {
        reasons.push("filesSkipped".into());
    }
    let result_page_limit = !snapshot_changed
        && base["pagination"]["hasMore"] == true
        && q.page() as usize >= PAGE_MAXIMUM;
    let diagnostic_page_limit = !snapshot_changed
        && base["coverage"]["diagnosticPagination"]["hasMore"] == true
        && q.diagnostic_page() as usize >= DIAGNOSTIC_PAGE_MAXIMUM;
    if result_page_limit {
        reasons.push("pageLimit".into());
    }
    if diagnostic_page_limit {
        base["coverage"]["diagnosticPagination"]["terminalLimit"] = json!(true);
    }
    // A skipped file is named once: `partialReasons` says the scan is
    // partial and the coverage diagnostics name the file and why.
    if !reasons.is_empty() {
        base.insert("isPartial".into(), json!(true));
        base.insert("partialReasons".into(), json!(reasons));
    }
    let terminal = result_page_limit
        || diagnostic_page_limit
        || b.files_skipped > 0
        || q.max_files().is_some_and(|x| x >= 50_000) && b.truncated;
    if terminal {
        base.insert("terminalLimit".into(), json!(true));
    }
    result_page_limit
}

/// A diagnostic page keeps its diagnostics and their pagination only.
pub(super) fn keep_diagnostics_only(base: &mut Map<String, Value>) {
    base.insert("results".into(), json!([]));
    base.shift_remove("pagination");
    base.shift_remove("summary");
    if let Some(coverage) = base.get_mut("coverage").and_then(Value::as_object_mut) {
        coverage.retain(|key, _| matches!(key.as_str(), "diagnostics" | "diagnosticPagination"));
    }
}

/// Complete is the default: the block names only the states that are not,
/// plus the coverage gaps. Withheld diagnostic rows are an opt-in lead, not
/// a remaining page.
pub(super) fn add_completeness(
    base: &mut Map<String, Value>,
    b: &BuiltGraph,
    result_state: &str,
    gaps: &[&str],
) {
    let graph_state = if b.truncated {
        "scan-truncated"
    } else if !gaps.is_empty() {
        "coverage-incomplete"
    } else {
        "complete"
    };
    let diag_state = if base["coverage"]["diagnosticPagination"]["terminalLimit"] == true {
        "truncated"
    } else if base["coverage"]["diagnosticPagination"]["hasMore"] == true {
        "pageable"
    } else {
        "complete"
    };
    let mut completeness = Map::new();
    for (key, state) in [
        ("results", result_state),
        ("graph", graph_state),
        ("diagnostics", diag_state),
    ] {
        if state != "complete" {
            completeness.insert(key.into(), json!(state));
        }
    }
    if !completeness.is_empty() {
        base.insert("completeness".into(), Value::Object(completeness));
    }
}

pub(super) fn traversal(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
) -> Result<(Vec<Value>, Value, Vec<String>, bool), AstGraphError> {
    let raw = q.source().ok_or_else(|| {
        AstGraphError::new(
            "invalidGraphQuery",
            format!("{} requires source", q.analysis().as_str()),
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
        for (importer, module, line) in reexport_dependents(b, &file) {
            if importer == file || listed.contains(&importer) {
                continue;
            }
            let kinds = b
                .nodes
                .get(&importer)
                .and_then(|node| node.edges.get(&module))
                .cloned()
                .unwrap_or_default();
            // The import naming the re-exported item, not the first import
            // from the re-exporting module.
            items.push(json!({"file":importer,"distance":1,"via":module,"reexportVia":module,"edgeKinds":kinds,"importLine":line,"confidence":"syntactic"}));
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
        if item.get("importLine").is_none()
            && let Some(line) = first_import_line(b, importer, imported)
        {
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
    let summary = json!({"source":file,"condensationComponentCount":c.components.len(),"topologicalLayerCount":c.layers.len(),"transitiveEdgeCount":trans.len()});
    Ok((items, summary, vec![], false))
}

/// Files that use `target`'s items through a module re-exporting them:
/// `sync/mod.rs` has `pub use notify::Notify` and `sync/broadcast.rs` has
/// `use super::Notify`. A module re-exports what it imports or re-exports
/// from `target` (`export * from` and `pub use x::*` re-export every public
/// name); an importer of that module counts only when it names one of those
/// items, so importers of other items of a hub module are not dependents.
/// Returns `(importer, re-exporting module, line of the import naming the
/// item)` in file order.
pub(super) fn reexport_dependents(b: &BuiltGraph, target: &str) -> Vec<(String, String, u32)> {
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
            .then_some((module, import.line))
        });
        if let Some((module, line)) = module {
            out.push((importer.clone(), module.to_owned(), line));
        }
    }
    out
}

pub(super) fn path_analysis(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
) -> Result<(Vec<Value>, Value, Vec<String>, bool), AstGraphError> {
    let file = node_key(
        q.source().ok_or_else(|| {
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

pub(super) fn cycles(b: &BuiltGraph) -> (Vec<Value>, Value, Vec<String>, bool) {
    // A module tree is not a dependency cycle: `mod x;` plus the child's use
    // of its ancestor (`use super::…`) loop by construction. Cycles are found
    // on the use graph without those edges; the loops they alone close are
    // counted, not listed.
    let uses = dependency_graph(&b.nodes);
    let c = condense(&uses);
    let layers = layer_map(&c);
    let trans = find_transitive(&c);
    let runtime = runtime_graph(&uses);
    let runtime_cycles = scc(&runtime, true);
    let file_witnesses = CycleWitnesses::new(&uses);
    let runtime_witnesses = CycleWitnesses::new(&runtime);
    let mut items = Vec::new();
    for (id, files) in c.components.iter().enumerate() {
        if files.len() == 1 && !uses[&files[0]].edges.contains_key(&files[0]) {
            continue;
        }
        let members = files.iter().cloned().collect::<BTreeSet<_>>();
        let contained = runtime_cycles
            .iter()
            .filter(|x| x.iter().all(|f| members.contains(f)))
            .cloned()
            .collect::<Vec<_>>();
        let outgoing = c
            .edges
            .get(&id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        // Small facts first and `files` last: a cycle too large for one
        // response page shows its counts and runtime cycles before its files.
        let mut item = json!({"componentId":id,"fileCount":files.len(),"runtimeCycleCount":contained.len(),"runtimeCycle":!contained.is_empty()});
        if !contained.is_empty() {
            item["runtimeCycles"] = json!(contained);
            item["runtimeCycleEdges"] = json!(witness_edges(runtime_witnesses.witness(&members)));
        }
        item["cycleEdges"] = json!(witness_edges(file_witnesses.witness(&members)));
        item["edgeKinds"] = json!(collect_kinds(&uses, files));
        item["topologicalLayer"] = json!(layers.get(&id));
        item["outgoingComponents"] = json!(outgoing);
        item["confidence"] = json!("syntactic");
        item["files"] = json!(files);
        items.push(item);
    }
    let rc = items.iter().filter(|x| x["runtimeCycle"] == true).count();
    let ce = c.edges.values().map(BTreeSet::len).sum::<usize>();
    let count = items.len();
    let mut summary = json!({"cycleCount":count,"runtimeCycleCount":rc,"condensationComponentCount":c.components.len(),"condensationEdgeCount":ce,"topologicalLayerCount":c.layers.len(),"transitiveEdgeCount":trans.len()});
    let in_use_cycle = items
        .iter()
        .filter_map(|item| item["files"].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let module_tree = scc(&b.nodes, true)
        .iter()
        .filter(|files| !files.iter().any(|f| in_use_cycle.contains(f.as_str())))
        .count();
    if module_tree > 0 {
        summary["moduleTreeCycleCount"] = json!(module_tree);
    }
    (items, summary, vec![], false)
}

/// `file` resolves relative to the scanned `path`; name that rule and, when a
/// scanned file shares the requested suffix, the spelling that would match.
pub(super) fn missing_file_message(file: &str, nodes: &BTreeMap<String, Node>) -> String {
    let mut message = format!(
        "source is not in the scanned graph: {file}. `source` is relative to `path` \
         (or absolute under it), and must be a scanned source file"
    );
    if let Some(candidate) = suffix_candidate(file, nodes) {
        message.push_str(&format!("; did you mean `{candidate}`?"));
    }
    message
}

pub(super) fn suffix_candidate<'a>(
    file: &str,
    nodes: &'a BTreeMap<String, Node>,
) -> Option<&'a String> {
    let suffix = format!("/{file}");
    nodes
        .keys()
        .find(|key| key.ends_with(&suffix) || file.ends_with(&format!("/{key}")))
}

/// The missing-file error, with an executable `hints.retrySuffixMatch` on
/// the suffix candidate when one exists (another query: a lead, not a page).
pub(super) fn missing_file_error(
    q: &AstTopologyQuery,
    file: &str,
    nodes: &BTreeMap<String, Node>,
) -> AstGraphError {
    let mut error = AstGraphError::new("invalidGraphQuery", missing_file_message(file, nodes));
    if let Some(candidate) = suffix_candidate(file, nodes) {
        let mut query = clean_query(q);
        query["source"] = json!(candidate);
        error.next = Some(Box::new(json!({
            "retrySuffixMatch": Continuation::new(ToolId::AstTopology, query)
                .why("Retry with the scanned file that shares this path suffix.")
                .confidence("medium")
                .build()
        })));
    }
    error
}

/// A graph key for `raw`: relative to the scanned root, or absolute under it.
/// A workspace-relative spelling of a file under the root (leading components
/// that name the root's own tail) resolves to the same key.
pub(super) fn node_key(raw: &str, root: &Path, nodes: &BTreeMap<String, Node>) -> String {
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

pub(super) fn graph_file(f: &str, root: &Path) -> String {
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
/// The first import (or, failing one, re-export) statement in `importer`
/// that links `target`.
pub(super) fn first_import_line(b: &BuiltGraph, importer: &str, target: &str) -> Option<u32> {
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
        // A qualified path (`crate::b::f()`) is its own evidence line.
        .or_else(|| {
            facts
                .calls
                .iter()
                .filter(|call| call.target.as_deref() == Some(target))
                .map(|call| call.line)
                .min()
        })
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
            "mainGoal": "test", "reasoning":"test",
            "operation":"dependencies",
            "path":".",
            "source":"src/index.ts",
            "diagnosticPageSize":2
        }))
        .expect("graph query");
        let mut result = json!({
            "pagination":{"hasMore":false},
            "coverage":{"diagnosticPagination":{
                "hasMore":true,
                "outOfRange":false,
                "resultId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }},
            "results":[]
        })
        .as_object()
        .cloned()
        .expect("result object");

        add_next(&mut result, &query, Path::new("/repo"), false, None);

        assert_eq!(result["next"]["nextDiagnosticPage"]["tool"], "astTopology");
        assert_eq!(
            result["next"]["nextDiagnosticPage"]["query"]["queries"][0]["operation"],
            "dependencies"
        );
        assert_eq!(
            result["next"]["nextDiagnosticPage"]["query"]["queries"][0]["diagnosticSnapshot"],
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );

        // Counts-only default: withheld rows are an opt-in lead to page 1 of
        // the same snapshot.
        let mut counts_only = json!({"pagination":{"hasMore":false},"coverage":{},"results":[]})
            .as_object()
            .cloned()
            .expect("result object");
        add_next(
            &mut counts_only,
            &query,
            Path::new("/repo"),
            false,
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        );
        assert!(counts_only["next"].get("nextDiagnosticPage").is_none());
        let offered = &counts_only["next"]["readDiagnostics"]["query"]["queries"][0];
        assert_eq!(offered["diagnosticPage"], 1);
        assert_eq!(offered["diagnosticPageSize"], 2);
        assert_eq!(
            offered["diagnosticSnapshot"],
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
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
            "mainGoal": "test", "reasoning":"test",
            "operation":"dependencies",
            "path":"packages/pkg",
            "source":"lib/src/index.ts"
        }))
        .expect("graph query");
        let error = missing_file_error(&query, "lib/src/index.ts", &nodes);
        let next = error.next.expect("repair continuation");
        assert_eq!(next["retrySuffixMatch"]["tool"], "astTopology", "{next}");
        assert_eq!(
            next["retrySuffixMatch"]["query"]["queries"][0]["source"], "src/index.ts",
            "{next}"
        );
        assert_eq!(
            next["retrySuffixMatch"]["query"]["queries"][0]["path"], "packages/pkg",
            "{next}"
        );
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
