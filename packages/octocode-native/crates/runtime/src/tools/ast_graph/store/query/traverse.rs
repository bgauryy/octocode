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

    /// [`Graph::resolve`]; an ambiguous reference with more candidates than
    /// it lists offers `next`, the `find` command that pages them all.
    pub(super) fn resolve(
        &self,
        reference: &str,
        kind: Option<NodeKind>,
    ) -> Result<u32, GraphOutput> {
        self.g.resolve(reference, kind).map_err(|failure| {
            let mut out = *failure;
            if out.value["truncated"] == true {
                let find = QueryOptions {
                    op: "find".into(),
                    target: Some(reference.to_owned()),
                    workspace: self.options.workspace.clone(),
                    kind: kind.map(|kind| kind.as_str().to_owned()),
                    ..QueryOptions::default()
                };
                out.value["next"] = json!(command(&find, &self.g.id));
            }
            out
        })
    }

    pub(super) fn filter(&self, default: &[EdgeKind]) -> EdgeFilter {
        EdgeFilter {
            kinds: if self.flags.edge_kinds.is_empty() {
                default.to_vec()
            } else {
                self.flags.edge_kinds.clone()
            },
            weakest: self.flags.weakest,
        }
    }

    /// `deps`, `dependents`, `callers`, `callees`, `walk`.
    pub(super) fn traversal(&self) -> Result<GraphOutput, GraphOutput> {
        let (g, op) = (self.g, self.options.op.as_str());
        let root = self.target("a node reference")?;
        let node_kind = g.t.nodes[root as usize].kind;
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
            descendants(g, root)
        } else {
            vec![root]
        };
        let filter = self.filter(&default_edges);
        // A file's default dependents include the files that use its exports
        // through a re-exporting module; an explicit --edge walks only those
        // edges.
        let reexports =
            op == "dependents" && node_kind == NodeKind::File && self.flags.edge_kinds.is_empty();
        let page = walk(
            g,
            &seeds,
            &Walk {
                direction: self.flags.direction.unwrap_or(default_direction),
                filter: &filter,
                depth: self.flags.depth.unwrap_or(default_depth),
                kind: self.flags.kind,
                reexports,
            },
        );
        let mut extra = Map::new();
        extra.insert("target".into(), Value::Object(g.node_json(root)));
        extra.extend(edges_extra(&filter));
        if op == "dependents" && filter.kinds.contains(&EdgeKind::Imports) {
            let unlinked = unlinked_path_imports(g, root);
            if !unlinked.is_empty() {
                extra.insert("summary".into(), json!({"unlinkedPathImports": unlinked}));
            }
        }
        if filter.kinds.contains(&EdgeKind::Calls) {
            extra.insert("coverage".into(), g.coverage(root));
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
        let mut out = path(g, from, to, direction, &filter);
        if filter.kinds.contains(&EdgeKind::Calls) {
            out.value["coverage"] = g.coverage(from);
        }
        Ok(out)
    }

    pub(super) fn hubs(&self) -> Result<GraphOutput, GraphOutput> {
        let filter = self.filter(&[EdgeKind::Imports]);
        let rank_by = self.flags.direction.unwrap_or(Direction::In);
        let page = hubs(self.g, &filter, self.flags.kind, rank_by);
        let mut extra = edges_extra(&filter);
        extra.insert(
            "rankBy".into(),
            json!(if rank_by == Direction::Out {
                "out"
            } else {
                "in"
            }),
        );
        Ok(run_page(self.options, self.g, extra, page))
    }
}
