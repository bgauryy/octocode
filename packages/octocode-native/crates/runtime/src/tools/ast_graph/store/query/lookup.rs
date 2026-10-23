//! Graph lookups: node keys and JSON, reference resolution, neighbors and
//! re-export users; plus the edge filter a walk follows.
use super::*;

impl Graph {
    pub(super) fn key(&self, id: u32) -> &str {
        self.t.str(self.t.nodes[id as usize].key)
    }

    pub(super) fn node_json(&self, id: u32) -> Map<String, Value> {
        let node = &self.t.nodes[id as usize];
        let mut out = Map::new();
        out.insert("id".into(), json!(self.key(id)));
        out.insert("kind".into(), json!(node.kind.as_str()));
        let detail = self.t.str(node.detail);
        match node.kind {
            NodeKind::File => {
                if !detail.is_empty() {
                    out.insert("language".into(), json!(detail));
                }
                let roles = super::super::classify::role_names(node.flags);
                if !roles.is_empty() {
                    out.insert("roles".into(), json!(roles));
                }
            }
            NodeKind::Symbol => {
                out.insert("name".into(), json!(self.t.str(node.name)));
                out.insert("symbolKind".into(), json!(detail));
                out.insert("file".into(), json!(self.key(node.file)));
                out.insert("line".into(), json!(node.line));
                if node.end_line != node.line {
                    out.insert("endLine".into(), json!(node.end_line));
                }
                if node.flags & FLAG_EXPORTED != 0 {
                    out.insert("exported".into(), json!(true));
                }
                if node.flags & super::super::format::FLAG_TEST != 0 {
                    out.insert("test".into(), json!(true));
                }
            }
            NodeKind::Package => {
                out.insert("ecosystem".into(), json!(detail));
            }
        }
        out
    }

    pub(super) fn edge_fields(&self, out: &mut Map<String, Value>, edge: &EdgeRec) {
        out.insert("edge".into(), json!(edge.kind.as_str()));
        let via = self.t.str(edge.detail);
        if !via.is_empty() && edge.kind != EdgeKind::Contains {
            out.insert("via".into(), json!(via));
        }
        if edge.line != NONE {
            out.insert("edgeLine".into(), json!(edge.line));
        }
        if edge.kind == EdgeKind::Calls {
            out.insert("confidence".into(), json!(edge.confidence.as_str()));
        }
        if edge.test {
            out.insert("test".into(), json!(true));
        }
    }

    /// How complete call edges are for `node`'s language: the share of call
    /// sites naming an in-repo declaration that the syntax linker resolved.
    pub(super) fn coverage(&self, node: u32) -> Value {
        let file = self.t.nodes[node as usize].file;
        let language = if file == NONE {
            ""
        } else {
            self.t.str(self.t.nodes[file as usize].detail)
        };
        let recall = self.manifest["callRecallByLanguage"][language]
            .as_f64()
            .or_else(|| self.manifest["callInternalRecall"].as_f64())
            .unwrap_or(1.0);
        let mut coverage = json!({"language": language, "callInternalRecall": recall});
        if recall < 0.9 {
            coverage["warning"] = json!(format!(
                "Syntax-linked calls: about {:.0}% of {} call sites that name code in this repo are linked; \
                 method calls on values of unknown type are missing. Treat call results as a lower bound \
                 and confirm with lspSearch references.",
                recall * 100.0,
                if language.is_empty() { "all" } else { language }
            ));
        }
        coverage
    }

    /// The snapshot every answer names. A scan cut (the `maxFiles` bound,
    /// the edge cap, files skipped as unreadable or oversized) leaves every
    /// answer partial: `partialReasons` names the cuts, and `reingest` the
    /// widened ingest that lifts a file-scan cut (the others it cannot).
    pub(super) fn summary(&self) -> Value {
        let mut out = json!({"id": self.id, "root": self.manifest["root"], "createdAt": self.manifest["createdAt"]});
        let scan = &self.manifest["scan"];
        let truncated = scan["truncated"] == true;
        let reasons = [
            (truncated, "maxFiles"),
            (scan["edgesCapped"] == true, "edgeCap"),
            (
                scan["filesSkipped"].as_u64().unwrap_or(0) > 0,
                "filesSkipped",
            ),
        ]
        .into_iter()
        .filter_map(|(cut, reason)| cut.then_some(reason))
        .collect::<Vec<_>>();
        if !reasons.is_empty() {
            out["partialReasons"] = json!(reasons);
        }
        if truncated {
            out["reingest"] = json!(reingest_command(&self.manifest));
        }
        out
    }

    /// Resolves a user reference to exactly one node.
    pub(super) fn resolve(&self, reference: &str, kind: Option<NodeKind>) -> Result<u32, Failure> {
        let admit = |id: &u32| kind.is_none_or(|k| self.t.nodes[*id as usize].kind == k);
        let trimmed = reference.trim().trim_start_matches("./");
        let root = self.manifest["root"].as_str().unwrap_or_default();
        // The snapshot root is canonical; canonicalize the reference too so a
        // symlinked spelling (macOS `/var` → `/private/var`) still matches.
        let absolute = std::fs::canonicalize(trimmed).unwrap_or_else(|_| PathBuf::from(trimmed));
        let relative = absolute
            .strip_prefix(root)
            .ok()
            .map(octocode_engine::text::slash_path);
        for key in [Some(trimmed.to_owned()), relative].into_iter().flatten() {
            if let Some(id) = self.t.by_key(&key).filter(admit) {
                return Ok(id);
            }
        }
        let mut found = self
            .name_range(&trimmed.to_lowercase(), false)
            .filter(|id| self.t.str(self.t.nodes[*id as usize].name) == trimmed)
            .filter(admit)
            .collect::<Vec<_>>();
        if found.is_empty() && trimmed.contains(['.', '#']) {
            let suffix = format!("#{}", trimmed.trim_start_matches('#'));
            found = (0..self.t.nodes.len() as u32)
                .filter(|id| self.key(*id).ends_with(&suffix))
                .filter(admit)
                .collect();
        }
        if found.is_empty() {
            let suffix = format!("/{trimmed}");
            found = (0..self.t.nodes.len() as u32)
                .filter(|id| {
                    self.t.nodes[*id as usize].kind == NodeKind::File
                        && self.key(*id).ends_with(&suffix)
                })
                .filter(admit)
                .collect();
        }
        match found.as_slice() {
            [only] => Ok(*only),
            [] => {
                let suggestions = self
                    .name_range(&trimmed.to_lowercase(), true)
                    .filter(admit)
                    .take(5)
                    .map(|id| self.key(id).to_owned())
                    .collect::<Vec<_>>();
                let mut out = GraphOutput::error(
                    ErrorCode::NotFound,
                    format!(
                        "no node matches {reference:?}; try `octocode graph query find <text>`"
                    ),
                );
                if !suggestions.is_empty() {
                    out.value["suggestions"] = json!(suggestions);
                }
                Err(Box::new(out))
            }
            many => {
                // `--kind` narrows only candidates of several kinds.
                let first = self.t.nodes[many[0] as usize].kind;
                let narrow = if many
                    .iter()
                    .all(|id| self.t.nodes[*id as usize].kind == first)
                {
                    ""
                } else {
                    " (or --kind)"
                };
                let mut out = GraphOutput::error(
                    ErrorCode::InvalidInput,
                    format!(
                        "{} nodes match {reference:?}; pass one of the candidate ids{narrow}",
                        many.len()
                    ),
                );
                out.value["candidates"] = json!(
                    many.iter()
                        .take(MAX_CANDIDATES)
                        .map(|id| Value::Object(self.node_json(*id)))
                        .collect::<Vec<_>>()
                );
                if many.len() > MAX_CANDIDATES {
                    out.value["total"] = json!(many.len());
                    out.value["truncated"] = json!(true);
                }
                Err(Box::new(out))
            }
        }
    }

    /// Node ids whose lowercase name equals (or, with `prefix`, starts with)
    /// `lower`, via the sorted name index.
    pub(super) fn name_range<'a>(
        &'a self,
        lower: &'a str,
        prefix: bool,
    ) -> impl Iterator<Item = u32> + 'a {
        let lowered = |id: u32| self.t.str(self.t.nodes[id as usize].name).to_lowercase();
        let start = self
            .t
            .name_index
            .partition_point(|id| lowered(*id).as_str() < lower);
        self.t.name_index[start..]
            .iter()
            .copied()
            .take_while(move |id| {
                let name = lowered(*id);
                if prefix {
                    name.starts_with(lower)
                } else {
                    name == lower
                }
            })
    }

    pub(super) fn neighbors<'a>(
        &'a self,
        node: u32,
        direction: Direction,
        filter: &'a EdgeFilter,
    ) -> impl Iterator<Item = (u32, &'a EdgeRec)> + 'a {
        let out = (direction != Direction::In)
            .then(|| self.t.out(node).iter().map(|edge| (edge.dst, edge)))
            .into_iter()
            .flatten();
        let incoming = (direction != Direction::Out)
            .then(|| self.t.incoming(node).map(|edge| (edge.src, edge)))
            .into_iter()
            .flatten();
        out.chain(incoming).filter(|(_, edge)| filter.admits(edge))
    }
}

impl Graph {
    /// The files that use an export of `file` through a re-exporting
    /// module: `uses` edges resolved through a re-export into a symbol
    /// `file` declares, from another file.
    pub(super) fn reexport_users(&self, file: u32) -> impl Iterator<Item = (u32, &EdgeRec)> + '_ {
        contained_symbols(self, file)
            .into_iter()
            .flat_map(move |symbol| {
                self.t
                    .incoming(symbol)
                    .filter(move |edge| {
                        edge.kind == EdgeKind::Uses
                            && self.t.str(edge.detail) == "reexport"
                            && edge.src != file
                            && self.t.nodes[edge.src as usize].kind == NodeKind::File
                    })
                    .map(|edge| (edge.src, edge))
            })
    }
}

pub(super) struct EdgeFilter {
    pub(super) kinds: Vec<EdgeKind>,
    pub(super) weakest: Confidence,
    /// Admit test edges: traversals do; the production rankings (`hubs`,
    /// `cycles`, `stats`) do not.
    pub(super) tests: bool,
}

impl EdgeFilter {
    pub(super) fn admits(&self, edge: &EdgeRec) -> bool {
        self.kinds.contains(&edge.kind)
            && (edge.kind != EdgeKind::Calls || edge.confidence <= self.weakest)
            && (self.tests || !edge.test)
    }

    /// The same filter over production edges only.
    pub(super) fn production(mut self) -> Self {
        self.tests = false;
        self
    }
}
