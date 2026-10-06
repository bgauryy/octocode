//! astTopology reachability and dead-code liveness: entry points, live
//! declarations through bindings and re-exports, and dead clusters.

use super::{algorithms::*, analysis::*, graph::normalize, types::*};
use octocode_engine::graph::IMPORT_USE_MODULE;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) fn reachability(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    read: &dyn Fn(&std::path::Path) -> Option<String>,
) -> (Vec<Value>, Value, Vec<String>, bool) {
    let (roots, warnings, low) = entrypoints(b, q, read);
    if low && roots.is_empty() {
        return (
            vec![],
            json!({"entrypointsResolvedCount":0,"classifiedCount":0,"unclassifiedCount":b.nodes.len()}),
            warnings,
            true,
        );
    }
    let live = reachable(&b.nodes, &roots, false);
    // Unreachable files lead as individual rows; reachable files are packed
    // into `files` rows of one page each, so no file is dropped or repeated.
    let mut items = b
        .nodes
        .keys()
        .filter(|f| !live.contains(*f))
        .map(|f| json!({"file":f,"reachable":false,"confidence":"syntactic"}))
        .collect::<Vec<_>>();
    let reachable_files = b
        .nodes
        .keys()
        .filter(|f| live.contains(*f))
        .collect::<Vec<_>>();
    let chunk = q.page_size().clamp(1, super::topology_max("pageSize")) as usize;
    items.extend(
        reachable_files
            .chunks(chunk)
            .map(|files| json!({"files":files,"reachable":true,"confidence":"syntactic"})),
    );
    (
        items,
        json!({"entrypointsResolved":roots,"entrypointsResolvedCount":roots.len(),"reachableCount":live.len(),"unreachableCount":b.nodes.len()-live.len()}),
        warnings,
        low,
    )
}

pub(super) fn dead_code(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    read: &dyn Fn(&std::path::Path) -> Option<String>,
) -> (Vec<Value>, Value, Vec<String>, bool) {
    let (roots, mut warnings, low_entries) = entrypoints(b, q, read);
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
    let mut liveness = Liveness::new(b, &roots);
    let static_live = reachable(&b.nodes, &roots, true);
    let dynamic = liveness
        .live
        .difference(&static_live)
        .cloned()
        .collect::<Vec<_>>();
    if !dynamic.is_empty() {
        // The count is the disclosure; `summary.dynamicOnlyFiles` names each
        // file once.
        warnings.push(format!("{} file(s) reachable only through a dynamic import() (summary.dynamicOnlyFiles) — lower confidence than static analysis, verify with lspSearch before treating as proof.",dynamic.len()))
    }
    let (clusters, cluster_by) = dead_clusters(b, q, &liveness.live);
    let rows = dead_rows(b, q, &mut liveness, &cluster_by);
    let count = rows.len();
    let ccount = clusters.len();
    let mut summary = json!({"entrypointsResolved":roots,"entrypointsResolvedCount":roots.len(),"deadClusters":clusters,"deadClusterCount":ccount,"deadExportCount":count});
    if !dynamic.is_empty() {
        summary["dynamicOnlyFiles"] = json!(dynamic);
    }
    (
        rows,
        summary,
        warnings,
        low_entries || b.truncated || b.files_skipped > 0 || !b.diagnostics.is_empty(),
    )
}

/// What the entrypoints keep alive: files, credited bindings, re-export and
/// star-re-export routes, and the live declaration ids per file.
pub(super) struct Liveness<'b> {
    live: BTreeSet<String>,
    rootset: BTreeSet<String>,
    public: BTreeSet<String>,
    real: BTreeSet<String>,
    rex: BTreeMap<String, Vec<(String, String)>>,
    star: BTreeMap<String, Vec<String>>,
    live_ids: BTreeMap<&'b str, BTreeSet<String>>,
    path_named: BTreeSet<&'b str>,
}

impl<'b> Liveness<'b> {
    fn new(b: &'b BuiltGraph, roots: &[String]) -> Self {
        let live = reachable(&b.nodes, roots, false);
        let rootset = roots.iter().cloned().collect::<BTreeSet<_>>();
        let public = rootset
            .union(&b.namespace_targets)
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut real = BTreeSet::new();
        let mut rex: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for (f, ff) in live.iter().filter_map(|f| Some((f, b.facts.get(f)?))) {
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
        let (live_ids, path_named) =
            declaration_liveness(b, &live, &rootset, &public, &mut real, &rex, &star);
        Self {
            live,
            rootset,
            public,
            real,
            rex,
            star,
            live_ids,
            path_named,
        }
    }
}

/// Mutually referencing file clusters with no path from any entrypoint.
pub(super) fn dead_clusters(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    live: &BTreeSet<String>,
) -> (Vec<Value>, BTreeMap<String, usize>) {
    let mut cluster_by = BTreeMap::new();
    let mut clusters = Vec::new();
    for files in scc_unsorted(&b.nodes) {
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
    (clusters, cluster_by)
}

/// One row per exported declaration that no entrypoint keeps alive.
pub(super) fn dead_rows(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    liveness: &mut Liveness,
    cluster_by: &BTreeMap<String, usize>,
) -> Vec<Value> {
    let star_targets = liveness.star.keys().cloned().collect::<BTreeSet<_>>();
    let mut rows = Vec::new();
    for (file, ff) in &b.facts {
        if !q.include_tests().unwrap_or(true) && crate::content::is_test_path(file) {
            continue;
        }
        let rust = file.ends_with(".rs");
        let live_ids = liveness.live_ids.remove(file.as_str()).unwrap_or_else(|| {
            live_declarations(
                file,
                ff,
                &liveness.public,
                &liveness.real,
                &liveness.rex,
                &liveness.star,
                rust && liveness.rootset.contains(file),
            )
        });
        let file_live = liveness.live.contains(file);
        let exported_unreferenced = !liveness.rootset.contains(file)
            && !star_targets.contains(file)
            && !b.namespace_targets.contains(file);
        // A `mod x;` declaration is module structure: the child file's own
        // liveness is tracked separately.
        for d in ff
            .declarations
            .iter()
            .filter(|d| d.exported && !(rust && d.kind == "module"))
        {
            let mut row = if !file_live {
                if let Some(id) = cluster_by.get(file) {
                    json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"dead-cluster","clusterId":id})
                } else {
                    json!({"file":file,"name":d.name,"kind":d.kind,"line":d.line,"reason":"unreachable-file"})
                }
            } else if exported_unreferenced && !live_ids.contains(&d.id) {
                let via = if d
                    .public_names()
                    .iter()
                    .any(|n| liveness.rex.contains_key(&binding(file, n)))
                {
                    "reexport-chain"
                } else if rust && liveness.path_named.contains(d.name.as_str()) {
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
    rows
}

pub(super) fn binding(f: &str, n: &str) -> String {
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
/// A credit on a re-exporting file can make the origin's export live:
/// re-exporting file → the files it re-exports from.
pub(super) fn reexport_origins<'b>(
    b: &'b BuiltGraph,
    star: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<&'b str, BTreeSet<&'b str>> {
    let mut origins: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (file, ff) in &b.facts {
        for target in ff.reexports.iter().filter_map(|r| r.target.as_deref()) {
            origins.entry(file).or_default().insert(target);
        }
    }
    for (origin, reexporters) in star {
        if let Some((origin, _)) = b.facts.get_key_value(origin) {
            for reexporter in reexporters {
                if let Some((reexporter, _)) = b.facts.get_key_value(reexporter) {
                    origins.entry(reexporter).or_default().insert(origin);
                }
            }
        }
    }
    origins
}

/// Names a file binds: declarations, re-exports and imports.
pub(super) fn file_bound_names(b: &BuiltGraph, file: &str) -> BTreeSet<String> {
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
}

pub(super) fn declaration_liveness<'b>(
    b: &'b BuiltGraph,
    live: &BTreeSet<String>,
    rootset: &BTreeSet<String>,
    public: &BTreeSet<String>,
    real: &mut BTreeSet<String>,
    rex: &BTreeMap<String, Vec<(String, String)>>,
    star: &BTreeMap<String, Vec<String>>,
) -> (BTreeMap<&'b str, BTreeSet<String>>, BTreeSet<&'b str>) {
    let origins = reexport_origins(b, star);
    // Names each file binds, built once per file on first use.
    let mut bound_names: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut declares = |file: &'b str, name: &str| {
        bound_names
            .entry(file)
            .or_insert_with(|| file_bound_names(b, file))
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
pub(super) fn live_declarations(
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

/// `read` reads a manifest (`package.json`, `Cargo.toml`) through the
/// caller's path and content policy.
pub(super) fn entrypoints(
    b: &BuiltGraph,
    q: &AstTopologyQuery,
    read: &dyn Fn(&std::path::Path) -> Option<String>,
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
        if let Some(text) = read(&b.root.join("package.json"))
            && let Ok(v) = serde_json::from_str::<Value>(&text)
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
        infer_rust_roots(b, read, &mut roots, &mut seen_roots);
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
pub(super) fn push_unique(roots: &mut Vec<String>, seen: &mut BTreeSet<String>, file: String) {
    if seen.insert(file.clone()) {
        roots.push(file);
    }
}

/// Infer Rust crate entrypoints: `[[bin]]`/`[lib]` (and other) `path = "…"`
/// targets declared in a root `Cargo.toml`, plus the conventional
/// `src/main.rs`, `src/lib.rs`, and `src/bin/*.rs` targets (also matched for
/// workspace members via their path suffix). Only node keys that actually exist
/// in the scanned graph are added.
pub(super) fn infer_rust_roots(
    b: &BuiltGraph,
    read: &dyn Fn(&std::path::Path) -> Option<String>,
    roots: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
) {
    let has_rust = b.nodes.keys().any(|k| k.ends_with(".rs"));
    if !has_rust {
        return;
    }
    if let Some(text) = read(&b.root.join("Cargo.toml")) {
        for path in cargo_target_paths(&text) {
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
pub(super) fn is_rust_conventional_root(key: &str) -> bool {
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
pub(super) fn cargo_target_paths(content: &str) -> Vec<String> {
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
pub(super) fn infer_go_roots(b: &BuiltGraph, roots: &mut Vec<String>, seen: &mut BTreeSet<String>) {
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
pub(super) fn leaves_json(v: &Value, out: &mut Vec<String>) {
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
pub(super) fn source_equivalent(p: &str, nodes: &BTreeMap<String, Node>) -> Option<String> {
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
