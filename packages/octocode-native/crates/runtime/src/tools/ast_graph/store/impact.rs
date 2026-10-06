//! `graph query impact`: blast radius of a change.
//!
//! Two precisions, chosen by what changed:
//! - **File seeds** (a changed file) propagate like Jest `--findRelatedTests`
//!   and Nx `affected`: every importer is affected, transitively.
//! - **Symbol seeds** (a changed declaration) propagate only through the
//!   edges that bind that declaration (`calls`, `uses`, `inherits`), plus
//!   importers of its module that bind no name at all (side-effect,
//!   namespace, or unlinked imports), which stay conservatively affected.
//!
//! Each affected file carries its distance, the edge that reached it, and the
//! weakest edge confidence along that path. Manifest, lockfile, and compiler
//! config changes affect every file below their directory.
use super::classify::{ROLE_ENTRY, ROLE_TEST, role_names};
use super::format::{Confidence, EdgeKind, FLAG_TEST, GraphTables, NONE, NodeKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_LISTED: usize = 100;

/// Files whose change invalidates everything below their directory.
pub(crate) fn is_global_config(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lockb"
            | "bun.lock"
            | "Cargo.toml"
            | "Cargo.lock"
            | "go.mod"
            | "go.sum"
            | "pyproject.toml"
            | "setup.py"
            | "setup.cfg"
            | "poetry.lock"
            | "uv.lock"
            | "Pipfile.lock"
            | "build.gradle"
            | "build.gradle.kts"
            | "pom.xml"
            | "CMakeLists.txt"
            | "compile_commands.json"
    ) || (name.starts_with("tsconfig") && name.ends_with(".json"))
        || name == "jsconfig.json"
        || (name.starts_with("requirements") && name.ends_with(".txt"))
}

#[derive(Clone, Copy)]
struct Reach {
    depth: u32,
    /// Hops that crossed a component (package/crate/module) boundary.
    package_depth: u32,
    confidence: Confidence,
    /// Edge that first reached this node: `(from node, kind, detail)`.
    via: Option<(u32, EdgeKind, u32)>,
    types_only: bool,
}

pub(crate) struct Impact {
    pub summary: Value,
    pub rows: Vec<Value>,
}

/// `seeds` are resolved nodes; `global` are `(directory, changed file)`
/// pairs for config changes (`""` is the scan root).
pub(crate) fn run(
    t: &GraphTables,
    seeds: &[u32],
    global: &[(String, String)],
    max_depth: u32,
) -> Impact {
    let mut walk = Propagation::new(t);
    let symbol_mode = walk.seed_changes(seeds, global);
    let depth_limited = walk.propagate(max_depth);
    report(t, &walk.reach, seeds, global, symbol_mode, depth_limited)
}

/// Breadth-first spread of a change from its seeds to everything that
/// depends on them.
struct Propagation<'t> {
    t: &'t GraphTables,
    reach: Vec<Option<Reach>>,
    /// Files whose whole module is affected (propagate to every importer).
    whole: Vec<bool>,
    queue: VecDeque<u32>,
    component_dir: BTreeMap<u32, u32>,
}

impl<'t> Propagation<'t> {
    fn new(t: &'t GraphTables) -> Self {
        let n = t.nodes.len();
        Self {
            t,
            reach: vec![None; n],
            whole: vec![false; n],
            queue: VecDeque::new(),
            component_dir: t
                .components
                .iter()
                .map(|(node, dir, _, _)| (*node, *dir))
                .collect(),
        }
    }

    fn seed(&mut self, id: u32) {
        if self.reach[id as usize].is_none() {
            self.reach[id as usize] = Some(Reach {
                depth: 0,
                package_depth: 0,
                confidence: Confidence::High,
                via: None,
                types_only: false,
            });
            self.queue.push_back(id);
        }
        if self.t.nodes[id as usize].kind == NodeKind::File {
            self.whole[id as usize] = true;
        }
    }

    /// Seeds the changed nodes and config scopes; returns whether any seed
    /// is a symbol (symbol-precision impact).
    fn seed_changes(&mut self, seeds: &[u32], global: &[(String, String)]) -> bool {
        let t = self.t;
        let mut symbol_mode = false;
        for &id in seeds {
            match t.nodes[id as usize].kind {
                NodeKind::File => {
                    self.seed(id);
                    // Every declaration in a changed file is changed.
                    for (sym, node) in t.nodes.iter().enumerate() {
                        if node.kind == NodeKind::Symbol && node.file == id {
                            self.seed(sym as u32);
                        }
                    }
                }
                NodeKind::Symbol => {
                    symbol_mode = true;
                    self.seed(id);
                    // Members change with their container.
                    let mut stack = vec![id];
                    while let Some(parent) = stack.pop() {
                        for edge in t.out(parent) {
                            if edge.kind == EdgeKind::Contains
                                && t.nodes[edge.dst as usize].kind == NodeKind::Symbol
                            {
                                self.seed(edge.dst);
                                stack.push(edge.dst);
                            }
                        }
                    }
                }
                NodeKind::Package => {
                    // A dependency upgrade: its importers are the changed files.
                    for edge in t.incoming(id) {
                        if edge.kind == EdgeKind::Imports {
                            self.seed(edge.src);
                        }
                    }
                }
            }
        }
        for (dir, _) in global {
            for (id, node) in t.nodes.iter().enumerate() {
                let path = t.str(node.key);
                if node.kind == NodeKind::File
                    && (dir.is_empty() || path.starts_with(&format!("{dir}/")))
                {
                    self.seed(id as u32);
                }
            }
        }
        symbol_mode
    }

    /// Files that bind at least one name from a target file (`uses` edges),
    /// so a symbol-level change can skip them when the name is not affected.
    /// An importer "binds" a target when its import statement names
    /// symbols (a `uses` edge from the same import line, or into the
    /// target): then only those symbols, not the whole module, carry the
    /// change.
    fn binds(&self, importer: u32, target: u32, import_line: u32) -> bool {
        let t = self.t;
        t.out(importer).iter().any(|edge| {
            edge.kind == EdgeKind::Uses
                && (edge.dst == target
                    || t.nodes[edge.dst as usize].file == target
                    || (import_line != NONE && edge.line == import_line))
        })
    }

    /// A barrel only re-exports: changes flow through it by symbol, so its
    /// importers are affected only for the names they bind.
    fn is_barrel(&self, file: u32) -> bool {
        let t = self.t;
        let mut reexports = 0usize;
        for edge in t.out(file) {
            match edge.kind {
                EdgeKind::Imports if t.str(edge.detail).contains("reexport") => reexports += 1,
                EdgeKind::Imports => return false,
                EdgeKind::Contains if t.nodes[edge.dst as usize].kind == NodeKind::Symbol => {
                    return false;
                }
                _ => {}
            }
        }
        reexports > 0
    }

    /// Runs the walk; returns whether `max_depth` stopped it.
    fn propagate(&mut self, max_depth: u32) -> bool {
        let mut depth_limited = false;
        while let Some(id) = self.queue.pop_front() {
            let Some(current) = self.reach[id as usize] else {
                continue;
            };
            if current.depth >= max_depth {
                depth_limited = true;
                continue;
            }
            for step in self.dependents(id) {
                self.visit(id, current, step);
            }
        }
        depth_limited
    }

    /// The edges a change of `id` flows back along.
    fn dependents(&self, id: u32) -> Vec<Step> {
        let t = self.t;
        let node = &t.nodes[id as usize];
        let mut next = Vec::new();
        for edge in t.incoming(id) {
            let src_file = t.nodes[edge.src as usize].file;
            // Intra-file callers still matter (they change behavior).
            if src_file == node.file
                && node.kind == NodeKind::Symbol
                && !matches!(
                    edge.kind,
                    EdgeKind::Calls | EdgeKind::Uses | EdgeKind::Inherits
                )
            {
                continue;
            }
            let admit = match edge.kind {
                EdgeKind::Calls | EdgeKind::Uses | EdgeKind::Inherits => true,
                EdgeKind::Imports => {
                    node.kind == NodeKind::File
                        && (self.whole[id as usize] || !self.binds(edge.src, id, edge.line))
                }
                EdgeKind::Contains => false,
            };
            if admit {
                next.push(Step {
                    src: edge.src,
                    kind: edge.kind,
                    detail: edge.detail,
                    confidence: edge.confidence,
                    types_only: edge.kind == EdgeKind::Imports
                        && t.str(edge.detail).contains("type"),
                });
            }
        }
        // A symbol being affected touches its file (not the whole module).
        if node.kind == NodeKind::Symbol && node.file != NONE {
            next.push(Step {
                src: node.file,
                kind: EdgeKind::Contains,
                detail: NONE,
                confidence: Confidence::High,
                types_only: false,
            });
        }
        next
    }

    fn visit(&mut self, id: u32, current: Reach, step: Step) {
        let t = self.t;
        let src = step.src;
        let crosses = self.component_dir.get(&t.nodes[src as usize].file)
            != self.component_dir.get(&t.nodes[id as usize].file);
        let candidate = Reach {
            depth: if step.kind == EdgeKind::Contains {
                current.depth
            } else {
                current.depth + 1
            },
            package_depth: current.package_depth + u32::from(crosses),
            confidence: current.confidence.max(step.confidence),
            via: Some((id, step.kind, step.detail)),
            types_only: current.types_only || step.types_only,
        };
        let slot = &mut self.reach[src as usize];
        let better = slot.is_none_or(|old| {
            (candidate.depth, candidate.confidence) < (old.depth, old.confidence)
        });
        if better {
            *slot = Some(candidate);
            self.queue.push_back(src);
        }
        if step.kind == EdgeKind::Imports
            && t.nodes[src as usize].kind == NodeKind::File
            && !self.whole[src as usize]
            && !self.is_barrel(src)
        {
            // Importers re-export or wrap what they import.
            self.whole[src as usize] = true;
            self.queue.push_back(src);
        }
    }
}

/// One edge a change flows back along.
struct Step {
    src: u32,
    kind: EdgeKind,
    detail: u32,
    confidence: Confidence,
    types_only: bool,
}

/// Affected files as rows, sorted by distance, plus the summary lists.
fn report(
    t: &GraphTables,
    reach: &[Option<Reach>],
    seeds: &[u32],
    global: &[(String, String)],
    symbol_mode: bool,
    depth_limited: bool,
) -> Impact {
    let key = |id: u32| t.str(t.nodes[id as usize].key);
    let n = t.nodes.len();
    let mut files = (0..n as u32)
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
    let mut test_functions = (0..n as u32)
        .filter(|id| {
            let node = &t.nodes[*id as usize];
            node.kind == NodeKind::Symbol
                && node.flags & FLAG_TEST != 0
                && reach[*id as usize].is_some()
        })
        .map(|id| (reach[id as usize].map_or(0, |r| r.depth), key(id)))
        .collect::<Vec<_>>();
    test_functions.sort_unstable();
    let test_function_count = test_functions.len();
    for (_, test) in &test_functions {
        let file = test.split('#').next().unwrap_or(test);
        if !tests.contains(&file) {
            tests.push(file);
        }
    }
    let seed_keys = seeds.iter().map(|id| key(*id)).collect::<Vec<_>>();
    let bucket = |from: u32, to: u32| {
        files
            .iter()
            .filter(|(_, r)| (from..=to).contains(&r.depth))
            .take(MAX_LISTED)
            .map(|(id, _)| key(*id))
            .collect::<Vec<_>>()
    };
    let summary = json!({
        "changed": seed_keys,
        "configChanges": global.iter().map(|(dir, file)| json!({"file": file, "scope": if dir.is_empty() { "." } else { dir.as_str() }})).collect::<Vec<_>>(),
        "precision": if symbol_mode && global.is_empty() { "symbol" } else { "file" },
        "allAffected": global.iter().any(|(dir, _)| dir.is_empty()),
        "affectedFiles": files.len(),
        "byDepth": by_depth,
        "affectedEntrypoints": entries.iter().take(MAX_LISTED).collect::<Vec<_>>(),
        "testsToRun": tests.iter().take(MAX_LISTED).collect::<Vec<_>>(),
        "testCount": tests.len(),
        "testFunctions": test_functions.iter().take(MAX_LISTED).map(|(_, k)| *k).collect::<Vec<_>>(),
        "testFunctionCount": test_function_count,
        "willBreak": bucket(1, 1),
        "likely": bucket(2, 2),
        "shouldTest": bucket(3, u32::MAX),
        "components": components,
        "depthLimited": depth_limited,
        "note": "syntax graph: dynamic dispatch, reflection, and string-based loading are invisible; low-confidence rows came through name-matched calls",
    });
    Impact { summary, rows }
}

/// One affected file: its distance, risk bucket, and the edge that reached it.
fn impact_row(t: &GraphTables, id: u32, r: &Reach) -> Value {
    let key = |id: u32| t.str(t.nodes[id as usize].key);
    let flags = t.nodes[id as usize].flags;
    // GitNexus-style buckets: d1 will break, d2 likely affected, d3+ test.
    let risk = match r.depth {
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
