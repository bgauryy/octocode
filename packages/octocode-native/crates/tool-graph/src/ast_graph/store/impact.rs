//! graphQuery `impact`: blast radius of a change.
//!
//! Two precisions, chosen by what changed:
//! - **File seeds** (a changed file) propagate like Jest `--findRelatedTests`
//!   and Nx `affected`: every importer is affected, transitively.
//! - **Symbol seeds** (a changed declaration) propagate only through the
//!   edges that bind that declaration (`calls`, `uses`, `inherits`,
//!   `passes`). An importer whose linked references into the module name
//!   only other items is skipped. One with no linked name at all
//!   (side-effect, namespace, glob or unlinked imports) is a module-level
//!   `candidate`: listed, since it may reach the name, but neither proven
//!   affected nor expanded as if its whole module changed.
//!
//! A Rust module's parent (`mod x;`, a `contains` edge of detail `module`)
//! is reached as an unexpanded candidate that walks on only up its own
//! module chain: a library's crate root is named among the affected
//! entrypoints, while the parent's other importers are not implied.
//!
//! Each affected file carries its distance, the edge that reached it, and the
//! weakest edge confidence along that path. A change to a config linking
//! reads (manifest, workspace list, compiler config) affects every file below
//! its directory.
use super::super::classify::{ROLE_ENTRY, ROLE_TEST, role_names};
use super::format::{Confidence, EdgeKind, FLAG_TEST, GraphTables, NONE, NodeKind};
use super::tables::MODULE_DETAIL;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod report;
use report::*;

#[derive(Clone, Copy)]
struct Reach {
    depth: u32,
    /// Hops that crossed a component (package/crate/module) boundary.
    package_depth: u32,
    confidence: Confidence,
    /// Edge that first reached this node: `(from node, kind, detail)`.
    via: Option<(u32, EdgeKind, u32)>,
    types_only: bool,
    /// Reached only through a module-level import under symbol precision:
    /// it binds no linked name of the changed module, so it is listed as
    /// a candidate and not expanded.
    candidate: bool,
    /// Reached only as the parent that declares a changed module: a
    /// candidate that walks on only up its own module chain.
    module_only: bool,
    /// Seeded because the config change at this index of `global` covers
    /// it, not because it was edited.
    config: Option<usize>,
}

pub(crate) struct Impact {
    pub summary: Value,
    pub rows: Vec<Value>,
    /// The summary's lists, which it counts: each pages as the rows of its
    /// own `list` call.
    pub lists: Lists,
    /// When the depth bound left affected files unlisted: how many, and the
    /// bound whose walk lists every one.
    pub unlisted: Option<Unlisted>,
}

/// The lists an `impact` summary counts, each paged as the rows of the
/// same query with `list` set to its name: the entrypoint files among the
/// rows, every test file reached with its reached test functions, and the
/// changed paths outside the graph.
pub(crate) const LISTS: [&str; 3] = ["affectedEntrypoints", "testsToRun", "unmapped"];

/// The lists of a walk's report (`unmapped` is the change set's, not the
/// walk's).
pub(crate) struct Lists {
    /// Entrypoint file keys, in row order.
    pub entrypoints: Vec<Value>,
    /// `{path, functions?}` per test file, in reach order.
    pub tests: Vec<Value>,
}

/// Affected files a depth bound left out of the rows.
pub(crate) struct Unlisted {
    pub files: usize,
    /// The smallest bound that lists every file the change reaches.
    pub depth: u32,
}

/// `seeds` are resolved nodes; `global` are `(directory, changed file)`
/// pairs for config changes (`""` is the scan root). `max_depth` bounds the
/// listed rows; the tests to run are every test the change reaches at any
/// depth, so a bound never drops one.
///
/// The rows are the whole walk's, cut at `max_depth`: a file's depth, risk
/// and reaching edge are the same under every bound that lists it (a
/// candidate a deeper proven reach replaces is no row at a smaller bound,
/// it is counted unlisted).
pub(crate) fn run(
    t: &GraphTables,
    seeds: &[u32],
    global: &[(String, String)],
    max_depth: u32,
) -> Impact {
    let mut walk = Propagation::new(t);
    let symbol_mode = walk.seed_changes(seeds, global);
    walk.symbol_precision = symbol_mode && global.is_empty();
    walk.propagate();
    let whole = walk.reach;
    let is_file = |id: usize| t.nodes[id].kind == NodeKind::File;
    let listed = whole
        .iter()
        .map(|reach| reach.filter(|r| r.depth <= max_depth))
        .collect::<Vec<_>>();
    let beyond = whole
        .iter()
        .enumerate()
        .filter_map(|(id, reach)| reach.filter(|r| is_file(id) && r.depth > max_depth))
        .map(|r| r.depth)
        .collect::<Vec<_>>();
    let unlisted = beyond.iter().max().map(|deepest| Unlisted {
        files: beyond.len(),
        depth: *deepest,
    });
    let reached = Reached {
        listed: &listed,
        tests: &whole,
    };
    let (summary, rows, lists) =
        report(t, &reached, seeds, global, symbol_mode, unlisted.is_some());
    Impact {
        summary,
        rows,
        lists,
        unlisted,
    }
}

/// What a change reaches: within the depth bound (the listed rows), and
/// at any depth (the tests to run).
struct Reached<'r> {
    listed: &'r [Option<Reach>],
    tests: &'r [Option<Reach>],
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
    /// The summary's `precision:"symbol"`: import-only reach is a candidate.
    symbol_precision: bool,
    /// `(target file, referring file)` for every linked reference
    /// (`uses`, `calls`, `inherits`, `passes`) across files, built on first
    /// use.
    referrers: std::cell::OnceCell<BTreeSet<(u32, u32)>>,
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
            symbol_precision: false,
            referrers: std::cell::OnceCell::new(),
        }
    }

    fn seed(&mut self, id: u32) {
        self.seed_by(id, None);
    }

    /// Seeds `id` as changed, by the config change at `config` when set.
    fn seed_by(&mut self, id: u32, config: Option<usize>) {
        if self.reach[id as usize].is_none() {
            self.reach[id as usize] = Some(Reach {
                depth: 0,
                package_depth: 0,
                confidence: Confidence::High,
                via: None,
                types_only: false,
                candidate: false,
                module_only: false,
                config,
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
                    for (sym, node) in t.node_ids().zip(&t.nodes) {
                        if node.kind == NodeKind::Symbol && node.file == id {
                            self.seed(sym);
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
        // Edited files were seeded first: they stay changed, not covered.
        for (index, (dir, _)) in global.iter().enumerate() {
            for (id, node) in t.node_ids().zip(&t.nodes) {
                let path = t.str(node.key);
                if node.kind == NodeKind::File
                    && (dir.is_empty() || path.starts_with(&format!("{dir}/")))
                {
                    self.seed_by(id, Some(index));
                }
            }
        }
        symbol_mode
    }

    /// Files that bind at least one name from a target file, so a
    /// symbol-level change can skip them when the name is not affected. An
    /// importer "binds" a target when its import statement names symbols (a
    /// `uses` edge from the same import line, or into the target), or when
    /// any of its declarations has a linked reference into the target
    /// (`paging::push_warnings()` through a module import): then only those
    /// symbols, not the whole module, carry the change.
    ///
    /// One import that links several files of one directory (a Go package,
    /// a Java package glob) imports that package: a linked reference into
    /// any of its files binds the import, as one into the target does.
    fn binds(&self, importer: u32, target: u32, import_line: u32) -> bool {
        let t = self.t;
        let out = t.out(importer);
        out.iter().any(|edge| {
            edge.kind == EdgeKind::Uses
                && (edge.dst == target
                    || t.nodes[edge.dst as usize].file == target
                    || (import_line != NONE && edge.line == import_line))
        }) || self.referrers().contains(&(target, importer))
            || (import_line != NONE && {
                let dir =
                    |file: u32| crate::ast_graph::paths::parent(t.str(t.nodes[file as usize].key));
                let detail = out
                    .iter()
                    .find(|edge| {
                        edge.kind == EdgeKind::Imports
                            && edge.dst == target
                            && edge.line == import_line
                    })
                    .map(|edge| edge.detail);
                out.iter().any(|edge| {
                    edge.kind == EdgeKind::Imports
                        && edge.line == import_line
                        && Some(edge.detail) == detail
                        && edge.dst != target
                        && dir(edge.dst) == dir(target)
                        && self.referrers().contains(&(edge.dst, importer))
                })
            })
    }

    fn referrers(&self) -> &BTreeSet<(u32, u32)> {
        self.referrers.get_or_init(|| {
            let t = self.t;
            let file_of = |id: u32| match t.nodes[id as usize].kind {
                NodeKind::File => id,
                _ => t.nodes[id as usize].file,
            };
            t.edges
                .iter()
                .filter(|edge| {
                    matches!(
                        edge.kind,
                        EdgeKind::Uses | EdgeKind::Calls | EdgeKind::Inherits | EdgeKind::Passes
                    ) && t.nodes[edge.dst as usize].kind == NodeKind::Symbol
                })
                .map(|edge| (file_of(edge.dst), file_of(edge.src)))
                .filter(|(target, referrer)| target != referrer && *referrer != NONE)
                .collect()
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

    /// Runs the walk to every node the change reaches.
    fn propagate(&mut self) {
        while let Some(id) = self.queue.pop_front() {
            let Some(current) = self.reach[id as usize] else {
                continue;
            };
            if current.candidate && !current.module_only {
                continue;
            }
            for step in self.dependents(id) {
                if current.module_only && !self.declares_module(&step) {
                    continue;
                }
                self.visit(id, current, step);
            }
        }
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
                    EdgeKind::Calls | EdgeKind::Uses | EdgeKind::Inherits | EdgeKind::Passes
                )
            {
                continue;
            }
            let admit = match edge.kind {
                EdgeKind::Calls | EdgeKind::Uses | EdgeKind::Inherits | EdgeKind::Passes => true,
                EdgeKind::Imports => {
                    node.kind == NodeKind::File
                        && (self.whole[id as usize] || !self.binds(edge.src, id, edge.line))
                }
                // The parent module that declares this file (`mod x;`).
                EdgeKind::Contains => {
                    node.kind == NodeKind::File && t.str(edge.detail) == MODULE_DETAIL
                }
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

    /// Whether `step` is a parent module's `mod x;` declaration.
    fn declares_module(&self, step: &Step) -> bool {
        step.kind == EdgeKind::Contains
            && step.detail != NONE
            && self.t.str(step.detail) == MODULE_DETAIL
    }

    fn visit(&mut self, id: u32, current: Reach, step: Step) {
        let t = self.t;
        let src = step.src;
        let module = self.declares_module(&step);
        let crosses = self.component_dir.get(&t.nodes[src as usize].file)
            != self.component_dir.get(&t.nodes[id as usize].file);
        // Under symbol precision a file not changed whole reaches its
        // importer only by module: an unbound import, or a namespace/glob
        // `uses` of the file itself.
        let candidate = module
            || (self.symbol_precision
                && t.nodes[id as usize].kind == NodeKind::File
                && !self.whole[id as usize]
                && matches!(step.kind, EdgeKind::Imports | EdgeKind::Uses));
        let reached = Reach {
            depth: if step.kind == EdgeKind::Contains && !module {
                current.depth
            } else {
                current.depth + 1
            },
            package_depth: current.package_depth + u32::from(crosses),
            confidence: current.confidence.max(step.confidence),
            via: Some((id, step.kind, step.detail)),
            types_only: current.types_only || step.types_only,
            candidate,
            module_only: module,
            config: None,
        };
        let slot = &mut self.reach[src as usize];
        // Proven reach outranks a candidate at any depth.
        let rank = |r: &Reach| (r.candidate, r.depth, r.confidence);
        let better = slot.is_none_or(|old| rank(&reached) < rank(&old));
        if better {
            *slot = Some(reached);
            self.queue.push_back(src);
        }
        if step.kind == EdgeKind::Imports
            && !candidate
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
