//! Graph lookups: node keys and JSON, reference resolution, neighbors and
//! re-export users; plus the edge filter a walk follows.
use super::*;

impl Graph {
    pub(super) fn key(&self, id: u32) -> &str {
        self.t.str(self.t.nodes[id as usize].key)
    }

    pub(super) fn node_json(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Map<String, Value>, GraphOutput> {
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
                if let Ok(index) = self.t.declarations.binary_search_by_key(&id, |d| d.node) {
                    let decl = &self.t.declarations[index];
                    if let Ok(identity) =
                        serde_json::from_str::<Value>(self.t.str(decl.test_identity))
                        && !identity.is_null()
                    {
                        out.insert("testIdentity".into(), identity);
                    }
                    out.insert("occurrenceId".into(), json!(self.t.str(decl.occurrence)));
                    out.insert("sourceDigest".into(), json!(self.t.str(decl.source_digest)));
                    if decl.source_start != NONE {
                        out.insert(
                            "sourceRange".into(),
                            sources::range(octocode_engine::text::analysis::ByteRange::new(
                                decl.source_start,
                                decl.source_end,
                            ))?,
                        );
                        out.insert(
                            "sourceOrigin".into(),
                            self.origin(decl.source_origin, cancel)?,
                        );
                    }
                    if decl.selection_start != NONE {
                        out.insert(
                            "selectionByteRange".into(),
                            sources::range(octocode_engine::text::analysis::ByteRange::new(
                                decl.selection_start,
                                decl.selection_end,
                            ))?,
                        );
                        out.insert(
                            "selectionOrigin".into(),
                            self.origin(decl.selection_origin, cancel)?,
                        );
                    }
                    if let Ok(assessment) = serde_json::from_str(self.t.str(decl.assessment)) {
                        out.insert("parseAssessment".into(), assessment);
                    }
                }
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
        Ok(out)
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
