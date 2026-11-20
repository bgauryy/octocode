//! Target resolution and the traversal answers: deps, callers, walk, path
//! and hubs.
use super::*;

impl Query<'_> {
    pub(super) fn target(&self, required: &str) -> Result<u32, GraphOutput> {
        self.target_of_kind(required, self.flags.kind)
    }

    /// The node `target` names, preferring nodes of `kind`
    /// ([`Query::resolve_preferring`]).
    pub(super) fn target_of_kind(
        &self,
        required: &str,
        kind: Option<NodeKind>,
    ) -> Result<u32, GraphOutput> {
        let reference = self.options.target.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("`{}` needs {required}", self.options.op),
            )
        })?;
        self.resolve_preferring(reference, kind)
    }

    /// The node `reference` names. `kind` chooses among the nodes it names;
    /// a reference that names no node of that kind resolves without it, so a
    /// `kind` that filters an answer's rows never hides the node it starts
    /// from.
    fn resolve_preferring(
        &self,
        reference: &str,
        kind: Option<NodeKind>,
    ) -> Result<u32, GraphOutput> {
        match self.resolve(reference, kind) {
            Err(missing)
                if kind.is_some() && missing.value["errorCode"] == json!(ErrorCode::NotFound) =>
            {
                self.resolve(reference, None)
            }
            resolved => resolved,
        }
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
        let how = Walk {
            direction: self.flags.direction.unwrap_or(default_direction),
            filter: &filter,
            depth: self.flags.depth.unwrap_or(default_depth),
            kind: self.flags.kind,
            include_seeds: matches!(op, "callers" | "callees"),
            reexports,
            target: match &target {
                TraversalTarget::Node(root) => Some(*root),
                TraversalTarget::Directory { .. } => None,
            },
            lists: match view {
                Some("files") => WalkRows::Files,
                Some("modules") => WalkRows::Modules,
                _ => WalkRows::Nodes,
            },
            window,
        };
        let page = walk(g, &seeds, &how, self.cancel)?;
        let (hints, related) = if page.total == 0 {
            self.empty_walk_leads(&seeds, &how)?
        } else {
            (Vec::new(), Vec::new())
        };
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
        extra.extend(edges_extra(&filter, !self.flags.edge_kinds.is_empty()));
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
        // Argument coverage of the target's languages only: the snapshot's
        // other languages concern none of these rows. It qualifies passes
        // rows: stated when `passes` was asked by name (its absence is the
        // question) or a row of the page was reached through one.
        if filter.kinds.contains(&EdgeKind::Passes)
            && (self.flags.edge_kinds.contains(&EdgeKind::Passes)
                || page.rows.iter().any(|row| row["edge"] == "passes"))
        {
            let sources = match &target {
                TraversalTarget::Node(root) => vec![*root],
                TraversalTarget::Directory { files, .. } => files.clone(),
            };
            extra.insert("argumentCoverage".into(), g.argument_coverage(&sources));
        }
        let (hints, mut related) = (hints, related);
        // The manifest states each declared name's unresolved call sites, so
        // this costs no evidence read and holds for listed answers too.
        let mut unresolved = None;
        if op == "callers" && how.direction == Direction::In {
            let (named, lead) = self.unresolved_callers(&seeds)?;
            if named > 0 {
                // The `unresolved` lead (by callee name) lists those sites;
                // the note promises it so the served row keeps it.
                let reach = if lead.is_some() {
                    "related lists them by name; text search may find more"
                } else {
                    "text search may find more"
                };
                let (sites, verb) = call_sites(named);
                unresolved = Some(format!(
                    "{sites} {verb} the target but their receiver could not be resolved; {reach}"
                ));
                related.extend(lead);
            }
        }
        if op == "callees" && how.direction == Direction::Out {
            let unlinked = seeds
                .iter()
                .map(|seed| super::super::manifest::unresolved_calls_of(&g.manifest, *seed))
                .sum::<u64>();
            if unlinked > 0 {
                // The note promises the `unresolved` read scoped to the
                // target, so the served row keeps it.
                let (sites, verb) = call_sites(unlinked);
                let verb = if verb == "name" { "link" } else { "links" };
                unresolved = Some(format!(
                    "{sites} in the target {verb} no edge though their name is declared in this repository (unknown receiver, ambiguous or unbound); related lists them"
                ));
                let mut row = super::paging::query_row(self.options, &g.id);
                row.retain(|field, _| matches!(field.as_str(), "path" | "graph" | "target"));
                row.insert("operation".into(), json!("unresolved"));
                row.insert("edges".into(), json!(["calls"]));
                row.insert(
                    "reasons".into(),
                    json!(super::super::tables::INTERNAL_REASONS),
                );
                related.push(
                    octocode_runtime_base::response::result::Continuation::related(
                        octocode_runtime_base::id::ToolId::GraphQuery,
                        Value::Object(row),
                    )
                    .build(),
                );
            }
        }
        // A symbol's rows are syntax candidates: lspSearch checks them.
        if let TraversalTarget::Node(root) = &target {
            let check = match op {
                "callers" | "callees" => op,
                _ => "references",
            };
            related.extend(g.lsp_check(*root, check, self.cancel)?);
        }
        if !related.is_empty() {
            extra.insert("related".into(), Value::Array(related));
        }
        let mut out = run_page(self.options, g, extra, page);
        octocode_runtime_base::response::result::add_hints(&mut out.value, hints);
        if let Some(warning) = unresolved {
            octocode_runtime_base::response::paging::mark_partial(
                &mut out.value,
                octocode_runtime_base::response::paging::PartialReason::CallSitesUnresolved,
                &[warning],
            );
        }
        Ok(out)
    }

    /// The call sites no edge records whose callee spells one of `seeds`'
    /// names (`x.f(..)` for `f`): callers the walk cannot list, by name only,
    /// counted at build into the manifest. With the `unresolved` call that
    /// lists them, when there are any.
    fn unresolved_callers(&self, seeds: &[u32]) -> Result<(u64, Option<Value>), GraphOutput> {
        let g = self.g;
        let mut names = std::collections::BTreeSet::new();
        for seed in seeds {
            let name = g.text(g.t.nodes[*seed as usize].name, self.cancel)?;
            names.insert(super::super::manifest::spelled_name(&name).to_owned());
        }
        let count = names
            .iter()
            .map(|name| {
                g.manifest["unresolvedCallNames"][name]
                    .as_u64()
                    .unwrap_or(0)
            })
            .sum::<u64>();
        let lead = (count > 0 && names.len() == 1).then(|| {
            // The sites are scoped by what they call, not by who calls.
            let mut row = serde_json::Map::new();
            super::paging::pin(&mut row, self.options, &g.id);
            row.insert("operation".into(), json!("unresolved"));
            row.insert("symbolName".into(), json!(names.iter().next()));
            octocode_runtime_base::response::result::Continuation::related(
                octocode_runtime_base::id::ToolId::GraphQuery,
                Value::Object(row),
            )
            .build()
        });
        Ok((count, lead))
    }

    /// Why a walk listed nothing when the same walk lists rows without its
    /// `kind`, or at a weaker call confidence: a hint for each, and the call
    /// that lists the weaker confidence's rows. Counting walks run only for
    /// an empty answer and materialize no row.
    fn empty_walk_leads(
        &self,
        seeds: &[u32],
        how: &Walk,
    ) -> Result<(Vec<String>, Vec<Value>), GraphOutput> {
        let count = |filter: &EdgeFilter, kind: Option<NodeKind>| {
            let counting = Walk {
                filter,
                kind,
                lists: WalkRows::Nodes,
                window: 0..0,
                ..*how
            };
            walk(self.g, seeds, &counting, self.cancel).map(|page| page.total)
        };
        let (mut hints, mut related) = (Vec::new(), Vec::new());
        if let Some(kind) = how.kind {
            let reached = count(how.filter, None)?;
            if reached > 0 {
                hints.push(format!(
                    "kind {} excludes all {reached} nodes this walk reaches; omit kind to list them",
                    kind.as_str()
                ));
            }
        }
        if how.filter.kinds.contains(&EdgeKind::Calls) {
            for weaker in [Confidence::Medium, Confidence::Low] {
                if weaker <= how.filter.weakest {
                    continue;
                }
                let filter = EdgeFilter {
                    kinds: how.filter.kinds.clone(),
                    weakest: weaker,
                    tests: how.filter.tests,
                };
                let listed = count(&filter, how.kind)?;
                if listed == 0 {
                    continue;
                }
                hints.push(format!(
                    "confidence {} lists {listed} candidate nodes; confirm them with lspSearch",
                    weaker.as_str()
                ));
                let mut row = query_row(self.options, &self.g.id);
                row.insert("confidence".into(), json!(weaker.as_str()));
                related.push(
                    octocode_runtime_base::response::result::Continuation::related(
                        octocode_runtime_base::id::ToolId::GraphQuery,
                        Value::Object(row),
                    )
                    .build(),
                );
                break;
            }
        }
        Ok((hints, related))
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
        let to = self.resolve_preferring(reference, self.flags.kind)?;
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
        if out.value["found"] == false {
            let wider = self.wider_paths(direction, &filter);
            if !wider.is_empty() {
                out.value["related"] = Value::Array(wider);
            }
        }
        Ok(out)
    }

    /// The walks a path the request did not find may take: this request
    /// with direction both, and with edges imports and calls, each that
    /// differs from what the request already walked.
    fn wider_paths(&self, direction: Direction, filter: &EdgeFilter) -> Vec<Value> {
        let mut calls = Vec::new();
        let mut wider = |field: &str, value: Value| {
            let mut row = query_row(self.options, &self.g.id);
            row.insert(field.into(), value);
            calls.push(
                octocode_runtime_base::response::result::Continuation::related(
                    octocode_runtime_base::id::ToolId::GraphQuery,
                    Value::Object(row),
                )
                .build(),
            );
        };
        if direction != Direction::Both {
            wider("direction", json!("both"));
        }
        let both = [EdgeKind::Imports, EdgeKind::Calls];
        if !both.iter().all(|kind| filter.kinds.contains(kind)) {
            wider("edges", json!(both.map(EdgeKind::as_str)));
        }
        calls
    }

    pub(super) fn hubs(&self) -> Result<GraphOutput, GraphOutput> {
        // Symbols import nothing: a symbol ranking defaults to its calls, as
        // a symbol's deps and dependents do.
        let natural = if self.flags.kind == Some(NodeKind::Symbol) {
            EdgeKind::Calls
        } else {
            EdgeKind::Imports
        };
        let filter = self.filter(&[natural]).production();
        let direction = self.flags.direction.unwrap_or(Direction::In);
        let page = hubs(
            self.g,
            &filter,
            self.flags.kind,
            direction,
            self.flags.rank_by,
            self.window(),
            self.cancel,
        )?;
        // What the ranking counts is one static fact of the walk: stated once,
        // in `rankBy` and `testEdges`, with the first page (a fresh request's
        // `edges` and defaulted `confidence` are dropped as echoes of its
        // query), never repeated by the pages after it.
        let mut extra = Map::new();
        if first_page(self.options) {
            extra.insert(
                "rankBy".into(),
                json!(ranking(&filter, direction, self.flags.rank_by)),
            );
            extra.insert("testEdges".into(), json!("excluded"));
        }
        Ok(run_page(self.options, self.g, extra, page))
    }
}

/// What a `hubs` page ranks by: the row value it sorts on, the edge kinds
/// counted (reversed for a PageRank over outgoing edges), what weights them
/// and, when calls are among them, the weakest call confidence.
fn ranking(filter: &EdgeFilter, direction: Direction, rank_by: RankBy) -> String {
    let kinds = filter
        .kinds
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let ranked = match (rank_by, direction) {
        (RankBy::Degree, Direction::Out) => format!("fanOut of {kinds}"),
        (RankBy::Degree, _) => format!("fanIn of {kinds}"),
        (RankBy::PageRank, Direction::Out) => {
            format!("pageRank of reversed {kinds} weighted by sites")
        }
        (RankBy::PageRank, _) => format!("pageRank of {kinds} weighted by sites"),
    };
    if filter.kinds.contains(&EdgeKind::Calls) {
        format!("{ranked} (confidence {})", filter.weakest.as_str())
    } else {
        ranked
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
        let file = row["path"].as_str().unwrap_or_default();
        *files.entry(file.to_owned()).or_default() += 1;
    }
    let rows = files
        .into_iter()
        .map(|(file, edges)| json!({"path": file, "edges": edges}))
        .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
        start: 0,
    }
}

/// `count` call sites as words, with the verb `name` that agrees with them.
fn call_sites(count: u64) -> (String, &'static str) {
    let verb = if count == 1 { "names" } else { "name" };
    (
        octocode_engine::text::counted(count, "call site", "call sites"),
        verb,
    )
}
