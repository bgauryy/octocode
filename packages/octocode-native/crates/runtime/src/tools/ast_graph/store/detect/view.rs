//! The precomputed file-level view every detector reads.
use super::*;

/// Precomputed file-level views shared by every detector.
pub(super) struct View<'a> {
    pub(super) t: &'a GraphTables,
    /// File node ids in key order.
    pub(super) files: Vec<u32>,
    /// Node → owning file node.
    pub(super) file_of: Vec<u32>,
    /// Runtime import adjacency between authored files, production edges
    /// only (sorted, distinct). Edges within one Rust module tree are left
    /// out ([`rust_lineage`]): they draw the tree, not a dependency.
    pub(super) out: Vec<Vec<u32>>,
    pub(super) inc: Vec<Vec<u32>>,
    /// First importing line per runtime edge.
    pub(super) edge_line: BTreeMap<(u32, u32), u32>,
    /// File → (component dir, name, meta `ecosystem[;library][;templates]`).
    pub(super) component: BTreeMap<u32, (&'a str, &'a str, &'a str)>,
    /// Entrypoints by rule: the inferred ones, or the caller's (`explicit`).
    pub(super) entries: BTreeMap<u32, &'a str>,
    pub(super) rank: Vec<f64>,
    pub(super) rank_percentile: Vec<f64>,
}

impl<'a> View<'a> {
    /// The view of `t`; `entries` (file keys) replace the inferred
    /// entrypoints.
    pub(super) fn new(t: &'a GraphTables, entries: Option<&[String]>) -> Self {
        let n = t.nodes.len();
        let files = (0..n as u32)
            .filter(|id| t.nodes[*id as usize].kind == NodeKind::File)
            .collect::<Vec<_>>();
        let file_of = t.nodes.iter().map(|node| node.file).collect::<Vec<_>>();
        let mut out = vec![Vec::new(); n];
        let mut inc = vec![Vec::new(); n];
        let mut edge_line = BTreeMap::new();
        for edge in &t.edges {
            // Test code is no production dependency.
            if edge.kind != EdgeKind::Imports || edge.src == edge.dst || edge.test {
                continue;
            }
            let (src, dst) = (&t.nodes[edge.src as usize], &t.nodes[edge.dst as usize]);
            if src.kind != NodeKind::File || dst.kind != NodeKind::File {
                continue;
            }
            if (src.flags | dst.flags) & (ROLE_NOT_AUTHORED | ROLE_DECLARATION) != 0 {
                continue;
            }
            // Type-only and deferred (dynamic, on-call) imports are no
            // evidence of a structural violation.
            if matches!(load(t.str(edge.detail)), Load::TypeOnly | Load::Deferred) {
                continue;
            }
            // A Rust module and the modules below it are one module tree:
            // `use super::x` and `use child::y` between them are no
            // structural coupling (hubs, cycles, placement).
            if rust_lineage(t, edge.src, edge.dst) {
                continue;
            }
            out[edge.src as usize].push(edge.dst);
            inc[edge.dst as usize].push(edge.src);
            let line = edge_line.entry((edge.src, edge.dst)).or_insert(edge.line);
            *line = (*line).min(edge.line);
        }
        for list in out.iter_mut().chain(inc.iter_mut()) {
            list.sort_unstable();
            list.dedup();
        }
        let component = t
            .components
            .iter()
            .map(|(node, dir, name, meta)| (*node, (t.str(*dir), t.str(*name), t.str(*meta))))
            .collect();
        let entries = match entries {
            Some(keys) => keys
                .iter()
                .filter_map(|key| t.by_key(key))
                .map(|node| (node, "explicit"))
                .collect(),
            None => t
                .entries
                .iter()
                .map(|(node, rule)| (*node, t.str(*rule)))
                .collect(),
        };
        let mut view = Self {
            t,
            files,
            file_of,
            out,
            inc,
            edge_line,
            component,
            entries,
            rank: Vec::new(),
            rank_percentile: Vec::new(),
        };
        view.rank = view.page_rank();
        view.rank_percentile = view.percentiles(&view.rank);
        view
    }

    pub(super) fn key(&self, id: u32) -> &'a str {
        self.t.str(self.t.nodes[id as usize].key)
    }
    pub(super) fn is_entry(&self, file: u32) -> bool {
        self.entries.contains_key(&file)
    }
    pub(super) fn flags(&self, file: u32) -> u8 {
        self.t.nodes[file as usize].flags
    }
    pub(super) fn language(&self, file: u32) -> &'a str {
        self.t.str(self.t.nodes[file as usize].detail)
    }
    pub(super) fn authored(&self, file: u32) -> bool {
        // Unparsed (oversized) files have no facts to judge.
        self.flags(file) & (ROLE_NOT_AUTHORED | ROLE_DECLARATION | ROLE_UNPARSED) == 0
    }
    /// A file that may be a subject of a production-code finding.
    /// Go/JVM/.NET files share a package scope without importing each
    /// other, so file-level structure (reachability, hubs, file cycles) is
    /// not observable for them; package-level detectors still apply.
    pub(super) fn package_scoped(&self, file: u32) -> bool {
        PACKAGE_SCOPED.contains(&self.language(file))
    }
    /// A production file whose file-level structure the graph observes.
    pub(super) fn file_level(&self, file: u32) -> bool {
        self.production(file) && !self.package_scoped(file)
    }
    pub(super) fn meta(&self, file: u32) -> &'a str {
        self.component.get(&file).map_or("", |(_, _, meta)| *meta)
    }
    pub(super) fn production(&self, file: u32) -> bool {
        self.authored(file) && self.flags(file) & (ROLE_TEST | ROLE_CONFIG) == 0
    }
    pub(super) fn impact(&self, file: u32) -> f64 {
        self.rank_percentile
            .get(file as usize)
            .copied()
            .unwrap_or(0.5)
    }
    pub(super) fn line(&self, src: u32, dst: u32) -> Option<u32> {
        self.edge_line
            .get(&(src, dst))
            .copied()
            .filter(|l| *l != NONE)
    }

    /// PageRank over runtime imports (importer → imported): a file many
    /// important files depend on ranks high. Damping 0.85, ≤ 50 iterations.
    pub(super) fn page_rank(&self) -> Vec<f64> {
        let n = self.t.nodes.len();
        let count = self.files.len().max(1) as f64;
        let mut rank = vec![0.0; n];
        for file in &self.files {
            rank[*file as usize] = 1.0 / count;
        }
        for _ in 0..50 {
            let mut next = vec![0.0; n];
            let mut dangling = 0.0;
            for file in &self.files {
                let targets = &self.out[*file as usize];
                let r = rank[*file as usize];
                if targets.is_empty() {
                    dangling += r;
                } else {
                    let share = r / targets.len() as f64;
                    for target in targets {
                        next[*target as usize] += share;
                    }
                }
            }
            let base = (1.0 - 0.85) / count + 0.85 * dangling / count;
            let mut delta = 0.0;
            for file in &self.files {
                let value = base + 0.85 * next[*file as usize];
                delta += (value - rank[*file as usize]).abs();
                next[*file as usize] = value;
            }
            rank = next;
            if delta < 1e-6 {
                break;
            }
        }
        rank
    }

    pub(super) fn percentiles(&self, values: &[f64]) -> Vec<f64> {
        let mut order = self.files.clone();
        order.sort_by(|a, b| {
            values[*a as usize]
                .total_cmp(&values[*b as usize])
                .then(a.cmp(b))
        });
        let mut out = vec![0.5; self.t.nodes.len()];
        let denominator = (order.len().max(2) - 1) as f64;
        for (position, file) in order.iter().enumerate() {
            out[*file as usize] = position as f64 / denominator;
        }
        out
    }

    /// Files transitively depending on `file` through runtime imports
    /// (modules of one Rust module tree excluded, as everywhere in the view).
    pub(super) fn dependents(&self, file: u32) -> usize {
        let mut seen = BTreeSet::from([file]);
        let mut queue = VecDeque::from([file]);
        while let Some(id) = queue.pop_front() {
            for next in &self.inc[id as usize] {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        seen.len() - 1
    }

    /// Node-level forward reachability over every edge kind from `seeds`;
    /// reaching a symbol also reaches (loads) its file. Test edges are
    /// followed only with `tests`: production code is not kept alive by
    /// the test code beside it.
    pub(super) fn reach(&self, seeds: &[u32], tests: bool) -> Vec<bool> {
        let mut seen = vec![false; self.t.nodes.len()];
        let mut queue = VecDeque::new();
        for seed in seeds {
            if !seen[*seed as usize] {
                seen[*seed as usize] = true;
                queue.push_back(*seed);
            }
        }
        while let Some(id) = queue.pop_front() {
            let file = self.file_of[id as usize];
            let mut visit = |next: u32, queue: &mut VecDeque<u32>| {
                if next != NONE && !seen[next as usize] {
                    seen[next as usize] = true;
                    queue.push_back(next);
                }
            };
            visit(file, &mut queue);
            for edge in self.t.out(id).iter().filter(|edge| tests || !edge.test) {
                visit(edge.dst, &mut queue);
            }
        }
        seen
    }
}

/// Whether `a` and `b` are Rust files of one module tree: one is a module
/// declared below the other (`tree.rs` or `tree/mod.rs` and `tree/child.rs`;
/// a crate root and any module of its crate). Their `use` edges are the
/// module tree, not coupling: no hub, cycle or placement evidence.
pub(super) fn rust_lineage(t: &GraphTables, a: u32, b: u32) -> bool {
    let (na, nb) = (&t.nodes[a as usize], &t.nodes[b as usize]);
    if t.str(na.detail) != "rust" || t.str(nb.detail) != "rust" {
        return false;
    }
    let below = |parent: &str, child: &str| {
        let dir = rust_module_child_dir(parent);
        dir == "."
            || child
                .strip_prefix(dir.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    };
    let (ka, kb) = (t.str(na.key), t.str(nb.key));
    below(ka, kb) || below(kb, ka)
}
