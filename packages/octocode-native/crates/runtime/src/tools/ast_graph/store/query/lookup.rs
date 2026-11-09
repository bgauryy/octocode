//! Graph lookups: node keys and JSON, reference resolution, neighbors and
//! re-export users; plus the edge filter a walk follows.
use super::*;

impl Graph {
    pub(super) fn node_record(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<super::super::format::NodeRec, GraphOutput> {
        if let Some(reader) = &self.selective
            && reader.borrow().is_point()
        {
            reader
                .borrow_mut()
                .node(id, &self.t, cancel)
                .map_err(|message| *decode_failure(message, cancel))
        } else {
            Ok(self.t.nodes[id as usize])
        }
    }

    fn declaration(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Option<super::super::format::DeclarationRec>, GraphOutput> {
        if let Some(reader) = &self.selective {
            reader
                .borrow_mut()
                .declaration(id, &self.t, cancel)
                .map_err(|message| *decode_failure(message, cancel))
        } else {
            Ok(self
                .t
                .declarations
                .binary_search_by_key(&id, |d| d.node)
                .ok()
                .map(|index| self.t.declarations[index]))
        }
    }

    pub(super) fn key(&self, id: u32) -> &str {
        self.t.str(self.t.nodes[id as usize].key)
    }

    /// The file a node lies in: a symbol's file; a file or package itself.
    pub(super) fn file_of(&self, id: u32) -> u32 {
        let node = &self.t.nodes[id as usize];
        match node.kind {
            NodeKind::Symbol => node.file,
            NodeKind::File | NodeKind::Package => id,
        }
    }

    /// The module a node belongs to: the root-relative directory of its file
    /// (`.` for the root); a package is its own module.
    pub(super) fn module(&self, id: u32) -> &str {
        if self.t.nodes[id as usize].kind == NodeKind::Package {
            return self.key(id);
        }
        self.key(self.file_of(id))
            .rsplit_once('/')
            .map_or(".", |(dir, _)| dir)
    }

    /// The file part of a symbol key `<file>#<qualified name>`: the longest
    /// `#`-prefix that is a file node's key, since a file path may hold `#`
    /// (`x.ts#y.ts#f` lies in `x.ts#y.ts` beside a file `x.ts`), while a
    /// name holds one only after its file (`mod.rs#r#type`). Else the text
    /// before the first `#`; `None` for a key with no `#`.
    pub(super) fn file_key<'k>(&self, key: &'k str) -> Option<&'k str> {
        let cuts = key
            .match_indices('#')
            .filter_map(|(at, _)| key.get(..at))
            .collect::<Vec<_>>();
        let first = *cuts.first()?;
        let is_file = |file: &&str| {
            self.t
                .by_key(file)
                .is_some_and(|id| self.t.nodes[id as usize].kind == NodeKind::File)
        };
        Some(cuts.into_iter().rev().find(is_file).unwrap_or(first))
    }

    pub(super) fn node_json(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Map<String, Value>, GraphOutput> {
        let node = self.node_record(id, cancel)?;
        let mut out = Map::new();
        out.insert("id".into(), json!(self.key(id)));
        out.insert("kind".into(), json!(node.kind.as_str()));
        let detail = self.t.str(node.detail);
        match node.kind {
            NodeKind::File => {
                if !detail.is_empty() {
                    out.insert("language".into(), json!(detail));
                }
                let roles = super::super::super::classify::role_names(node.flags);
                if !roles.is_empty() {
                    out.insert("roles".into(), json!(roles));
                }
            }
            NodeKind::Symbol => {
                // A point answer's keys come from lookup entries: the file's
                // is checked against its node record before it is rendered.
                self.node_record(node.file, cancel)?;
                if let Some(decl) = self.declaration(id, cancel)? {
                    let file = self.key(node.file);
                    let digest = self.t.str(decl.source_digest);
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
                            self.project_origin(decl.source_origin, file, digest, cancel)?
                                .value,
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
                            self.project_origin(decl.selection_origin, file, digest, cancel)?
                                .value,
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

    /// The lspSearch call that confirms a symbol's identity and uses at its
    /// declaration: graph rows are syntax candidates. The symbol name is the
    /// declared identifier (a member's last segment); `None` without a line.
    pub(super) fn lsp_lead(&self, file: &str, name: &str, line: u64) -> Option<Value> {
        if line == 0 || line >= u64::from(NONE) {
            return None;
        }
        let identifier = name
            .rsplit(['.', ':'])
            .find(|part| !part.is_empty())
            .unwrap_or(name);
        let row = json!({
            "operation": "references",
            "path": self.workspace_path(file),
            "symbolName": identifier,
            "line": line,
        });
        Some(
            crate::response::result::Continuation::related(crate::id::ToolId::LspSearch, row)
                .build(),
        )
    }

    /// The lspSearch call that confirms an edge row's edge: references of
    /// the symbol the edge points to, at its declaration. The edge holds
    /// when one of them lies at the row's `edgeLine` inside the symbol the
    /// edge leaves. `None` for a containment edge or an edge into a file or
    /// package: such a row keeps its own declaration's lead.
    pub(super) fn edge_lead(
        &self,
        edge: &EdgeRec,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Option<Value>, GraphOutput> {
        if edge.kind == EdgeKind::Contains {
            return Ok(None);
        }
        let target = self.node_record(edge.dst, cancel)?;
        if target.kind != NodeKind::Symbol {
            return Ok(None);
        }
        Ok(self.lsp_lead(
            self.key(target.file),
            self.t.str(target.name),
            u64::from(target.line),
        ))
    }

    /// A root-relative key as the workspace-relative path tools take;
    /// absolute when the snapshot records no workspace holding its root.
    pub(super) fn workspace_path(&self, key: &str) -> String {
        let root = Path::new(self.manifest["root"].as_str().unwrap_or_default());
        let base = self.manifest["workspace"]
            .as_str()
            .and_then(|workspace| root.strip_prefix(workspace).ok())
            .unwrap_or(root);
        octocode_engine::text::slash_path(&base.join(key))
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

    /// The snapshot every answer names. Files the scan skipped as
    /// unreadable or oversized leave every answer partial: `partialReasons`
    /// names the cut. A snapshot bound is no cut: a build past one is
    /// refused, never published.
    pub(super) fn summary(&self) -> Value {
        let mut out = json!({"id": self.id, "root": self.manifest["root"], "createdAt": self.manifest["createdAt"]});
        if self.manifest["scan"]["filesSkipped"].as_u64().unwrap_or(0) > 0 {
            crate::response::paging::mark_partial(
                &mut out,
                crate::response::paging::PartialReason::FilesSkipped,
                &[],
            );
        }
        out
    }

    /// Node ids whose lowercase name equals `lower`, via the sorted name index.
    pub(super) fn name_range<'a>(&'a self, lower: &'a str) -> impl Iterator<Item = u32> + 'a {
        let lowered = |id: u32| self.t.str(self.t.nodes[id as usize].name).to_lowercase();
        let start = self
            .t
            .name_index
            .partition_point(|id| lowered(*id).as_str() < lower);
        self.t.name_index[start..]
            .iter()
            .copied()
            .take_while(move |id| lowered(*id) == lower)
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
        self.admits_kind(edge) && !self.below_confidence(edge)
    }

    /// Admits the edge's kind and test role, whatever its confidence.
    pub(super) fn admits_kind(&self, edge: &EdgeRec) -> bool {
        self.kinds.contains(&edge.kind) && (self.tests || !edge.test)
    }

    /// A call edge weaker than the filter's confidence.
    pub(super) fn below_confidence(&self, edge: &EdgeRec) -> bool {
        edge.kind == EdgeKind::Calls && edge.confidence > self.weakest
    }

    /// The same filter over production edges only.
    pub(super) fn production(mut self) -> Self {
        self.tests = false;
        self
    }
}
