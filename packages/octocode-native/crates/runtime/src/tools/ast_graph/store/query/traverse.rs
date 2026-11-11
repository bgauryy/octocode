//! Target resolution and the traversal answers: deps, callers, walk, path
//! and hubs.
use super::*;

impl Query<'_> {
    pub(super) fn target(&self, required: &str) -> Result<u32, GraphOutput> {
        let reference = self.options.target.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("`{}` needs {required}", self.options.op),
            )
        })?;
        self.resolve(reference, self.flags.kind)
    }

    /// The roots of `symbols`: each source reference, else every file in
    /// key order, so no target lists every symbol of the snapshot.
    pub(super) fn symbol_roots(&self) -> Result<Vec<u32>, GraphOutput> {
        let sources = self.options.sources().collect::<Vec<_>>();
        if sources.is_empty() {
            let mut files = self
                .g
                .t
                .node_ids()
                .filter(|id| self.g.t.nodes[*id as usize].kind == NodeKind::File)
                .collect::<Vec<_>>();
            files.sort_by(|a, b| self.g.key(*a).cmp(self.g.key(*b)));
            return Ok(files);
        }
        sources
            .into_iter()
            .map(|reference| self.resolve(reference, self.flags.kind))
            .collect()
    }

    pub(super) fn filter(&self, default: &[EdgeKind]) -> EdgeFilter {
        EdgeFilter {
            kinds: if self.flags.edge_kinds.is_empty() {
                default.to_vec()
            } else {
                self.flags.edge_kinds.clone()
            },
            weakest: self.flags.weakest,
            tests: true,
        }
    }

    /// What a traversal starts from: one node, or for `deps` and
    /// `dependents` a directory that names no node, as every file under it.
    fn traversal_target(&self) -> Result<TraversalTarget, GraphOutput> {
        let directory = matches!(self.options.op.as_str(), "deps" | "dependents")
            && self.flags.kind.is_none_or(|kind| kind == NodeKind::File);
        if directory
            && let Some(reference) = self.options.target.as_deref()
            && let Some((path, files)) = self.g.directory(reference)
        {
            return Ok(TraversalTarget::Directory { path, files });
        }
        Ok(TraversalTarget::Node(self.target("a node reference")?))
    }

    /// `deps`, `dependents`, `callers`, `callees`, `walk`.
    pub(super) fn traversal(&self) -> Result<GraphOutput, GraphOutput> {
        let (g, op) = (self.g, self.options.op.as_str());
        let target = self.traversal_target()?;
        let (roots, node_kind) = match &target {
            TraversalTarget::Node(root) => (vec![*root], g.t.nodes[*root as usize].kind),
            TraversalTarget::Directory { files, .. } => (files.clone(), NodeKind::File),
        };
        let natural = if node_kind == NodeKind::Symbol {
            EdgeKind::Calls
        } else {
            EdgeKind::Imports
        };
        let (default_edges, default_direction, default_depth) = match op {
            "deps" => (vec![natural], Direction::Out, 1),
            "dependents" => (vec![natural], Direction::In, 1),
            "callers" => (vec![EdgeKind::Calls], Direction::In, 1),
            "callees" => (vec![EdgeKind::Calls], Direction::Out, 1),
            _ => (EdgeKind::ALL.to_vec(), Direction::Out, 2),
        };
        // A file's callers/callees are those of every symbol it declares.
        let seeds = if matches!(op, "callers" | "callees") && node_kind == NodeKind::File {
            roots
                .iter()
                .flat_map(|root| descendants(g, *root))
                .collect()
        } else {
            roots.clone()
        };
        let filter = self.filter(&default_edges);
        // A file's default dependents include the files that use its exports
        // through a re-exporting module; explicit edges walk only those
        // edges.
        let reexports =
            op == "dependents" && node_kind == NodeKind::File && self.flags.edge_kinds.is_empty();
        let view = self.options.view.as_deref();
        let grouped = matches!(view, Some("files" | "modules"));
        let offset = self.options.offset.unwrap_or(0);
        let limit = self.options.limit.unwrap_or_else(default_limit);
        // Grouping counts every edge before paging; the grouped rows page.
        let window = if grouped {
            0..usize::MAX
        } else {
            offset..offset.saturating_add(limit)
        };
        let page = walk(
            g,
            &seeds,
            &Walk {
                direction: self.flags.direction.unwrap_or(default_direction),
                filter: &filter,
                depth: self.flags.depth.unwrap_or(default_depth),
                kind: self.flags.kind,
                include_seeds: matches!(op, "callers" | "callees"),
                reexports,
                lists: match view {
                    Some("files") => WalkRows::Files,
                    Some("modules") => WalkRows::Modules,
                    _ => WalkRows::Nodes,
                },
                window,
            },
            self.cancel,
        )?;
        let page = match view {
            Some("files") => per_file(page),
            Some("modules") => per_module(page),
            _ => page,
        };
        let mut extra = Map::new();
        let target_json = match &target {
            TraversalTarget::Node(root) => Value::Object(g.node_json(*root, self.cancel)?),
            // A directory is no node: its root-relative path and its kind.
            TraversalTarget::Directory { path, .. } => json!({"id": path, "kind": "directory"}),
        };
        extra.insert("target".into(), target_json);
        extra.extend(edges_extra(&filter));
        if op == "dependents" && filter.kinds.contains(&EdgeKind::Imports) {
            let unlinked = roots
                .iter()
                .flat_map(|root| unlinked_path_imports(g, *root))
                .collect::<Vec<_>>();
            if !unlinked.is_empty() {
                extra.insert("summary".into(), json!({"unlinkedPathImports": unlinked}));
            }
        }
        if filter.kinds.contains(&EdgeKind::Calls) {
            let coverage = match &target {
                TraversalTarget::Node(root) => g.coverage(*root),
                TraversalTarget::Directory { files, .. } => g.files_coverage(files),
            };
            extra.insert("coverage".into(), coverage);
        }
        Ok(run_page(self.options, g, extra, page))
    }

    pub(super) fn path(&self) -> Result<GraphOutput, GraphOutput> {
        let g = self.g;
        let from = self.target("a source reference")?;
        let reference = self.options.to.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                "`path` needs a target: path <from> <to>",
            )
        })?;
        let to = self.resolve(reference, self.flags.kind)?;
        let both_symbols = g.t.nodes[from as usize].kind == NodeKind::Symbol
            && g.t.nodes[to as usize].kind == NodeKind::Symbol;
        let default = if both_symbols {
            vec![EdgeKind::Calls]
        } else {
            vec![EdgeKind::Imports]
        };
        let filter = self.filter(&default);
        let direction = self.flags.direction.unwrap_or(Direction::Out);
        let mut out = path(g, from, to, direction, &filter, self.cancel)?;
        out.value["interpretation"] = json!({"kind":"candidate","conditionCompatibility":"notAssessed","execution":"notObserved"});
        if filter.kinds.contains(&EdgeKind::Calls) {
            out.value["confidence"] = json!(filter.weakest.as_str());
            out.value["coverage"] = g.coverage(from);
        }
        Ok(out)
    }

    pub(super) fn hubs(&self) -> Result<GraphOutput, GraphOutput> {
        let filter = self.filter(&[EdgeKind::Imports]).production();
        let rank_by = self.flags.direction.unwrap_or(Direction::In);
        let page = hubs(
            self.g,
            &filter,
            self.flags.kind,
            rank_by,
            self.window(),
            self.cancel,
        )?;
        // What the ranking counts is one static fact of the walk: stated once,
        // in `rankBy`, with the first page (a fresh request's `edges` and
        // defaulted `confidence` are dropped as echoes of its query), never
        // repeated by the pages after it.
        let mut extra = Map::new();
        if first_page(self.options) {
            extra.insert("rankBy".into(), json!(ranking(&filter, rank_by)));
            if !filter.tests {
                extra.insert("testEdges".into(), json!("excluded"));
            }
        }
        Ok(run_page(self.options, self.g, extra, page))
    }
}

/// What a `hubs` page ranks by: the row count it sorts on, the edge kinds
/// counted and, when calls are among them, the weakest call confidence.
fn ranking(filter: &EdgeFilter, rank_by: Direction) -> String {
    let count = if rank_by == Direction::Out {
        "fanOut"
    } else {
        "fanIn"
    };
    let kinds = filter
        .kinds
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if filter.kinds.contains(&EdgeKind::Calls) {
        format!(
            "{count} of {kinds} (confidence {})",
            filter.weakest.as_str()
        )
    } else {
        format!("{count} of {kinds}")
    }
}

/// A traversal's start: one node, or a directory as the files under it.
enum TraversalTarget {
    Node(u32),
    Directory { path: String, files: Vec<u32> },
}

/// `view:"modules"`: one row per pair of directories the walked edges join
/// (`from` the importing or calling side, `to` the imported or called one)
/// with its edge count, in key order; the counts sum to the walked edges.
fn per_module(page: Page) -> Page {
    let mut pairs = BTreeMap::<(String, String), u64>::new();
    for row in &page.rows {
        let side = |field: &str| row[field].as_str().unwrap_or_default().to_owned();
        *pairs.entry((side("from"), side("to"))).or_default() += 1;
    }
    let rows = pairs
        .into_iter()
        .map(|((from, to), edges)| json!({"from": from, "to": to, "edges": edges}))
        .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
        start: 0,
    }
}

/// `view:"files"`: one row per file with the number of walked edges that
/// reach it or its symbols, in key order; the counts sum to the walked edges.
fn per_file(page: Page) -> Page {
    let mut files = BTreeMap::<String, u64>::new();
    for row in &page.rows {
        let file = row["file"].as_str().unwrap_or_default();
        *files.entry(file.to_owned()).or_default() += 1;
    }
    let rows = files
        .into_iter()
        .map(|(file, edges)| json!({"file": file, "edges": edges}))
        .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
        start: 0,
    }
}
