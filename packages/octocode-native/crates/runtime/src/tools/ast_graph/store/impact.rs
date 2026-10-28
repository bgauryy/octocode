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
