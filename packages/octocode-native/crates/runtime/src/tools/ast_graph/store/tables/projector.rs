//! Building the projector: file nodes and their symbol nodes.
use super::*;

impl<'a> Projector<'a> {
    /// File nodes, sorted by path.
    pub(super) fn new(
        built: &'a BuiltGraph,
        workspace: &'a Workspace,
        sources: sources::Sources,
    ) -> Self {
        let languages = built
            .nodes
            .keys()
            .map(String::as_str)
            .chain(
                built
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == DiagnosticCode::ScanSkip)
                    .map(|d| d.file.as_str()),
            )
            .map(|file| {
                let raw = built.facts.get(file).map_or("", |f| f.language.as_str());
                (file, canonical_language(file, raw))
            })
            .collect::<HashMap<_, _>>();
        let unparsed = built
            .diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::ScanSkip && !built.nodes.contains_key(&d.file))
            .map(|d| d.file.as_str())
            .collect::<BTreeSet<_>>();
        let mut projector = Self {
            sources,
            built,
            workspace,
            strings: Interner::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            witnesses: Vec::new(),
            unresolved: Vec::new(),
            declarations: Vec::new(),
            file_nodes: BTreeMap::new(),
            roles: HashMap::new(),
            languages,
            unparsed,
        };
        let mut all_files = built.nodes.keys().map(String::as_str).collect::<Vec<_>>();
        all_files.extend(projector.unparsed.iter().copied());
        all_files.sort_unstable();
        all_files.dedup();
        for file in all_files {
            projector.push_file(file);
        }
        projector.mount_test_modules();
        projector
    }

    pub(super) fn push_file(&mut self, file: &'a str) {
        let id = self.nodes.len() as u32;
        let name = file.rsplit('/').next().unwrap_or(file);
        let marks = self.built.facts.get(file).map(|facts| facts.marks);
        let mut flags = file_roles(&self.built.root, file, marks);
        if self.workspace.entries.contains_key(file) {
            flags |= ROLE_ENTRY;
        }
        if self.unparsed.contains(file) {
            flags |= ROLE_UNPARSED;
        }
        self.roles.insert(file, flags);
        let language = self.lang(file).to_owned();
        self.nodes.push(NodeRec {
            kind: NodeKind::File,
            flags,
            key: self.strings.id(file),
            name: self.strings.id(name),
            detail: self.strings.id(&language),
            file: id,
            parent: NONE,
            line: NONE,
            end_line: NONE,
        });
        self.file_nodes.insert(file, id);
    }

    pub(super) fn lang(&self, file: &str) -> &str {
        self.languages.get(file).map_or("", String::as_str)
    }

    pub(super) fn has_role(&self, file: &str, mask: u8) -> bool {
        self.roles.get(file).is_some_and(|r| r & mask != 0)
    }

    pub(super) fn push(
        &mut self,
        src: u32,
        dst: u32,
        kind: EdgeKind,
        confidence: Confidence,
        detail: &str,
        line: u32,
    ) {
        let test = self.test_evidence(src, line);
        let detail = self.strings.id(detail);
        let edge = EdgeRec {
            src,
            dst,
            kind,
            confidence,
            detail,
            line,
            test,
        };
        // This ordinal distinguishes repeated sites on the same source line.
        // Calls replace it with the extractor's source occurrence id below.
        let occurrence = self
            .strings
            .id(&format!("projection:{}", self.witnesses.len()));
        let provenance = self.provenance(src, None);
        self.witnesses.push(WitnessRec {
            argument: NONE,
            edge,
            occurrence,
            provenance,
        });
        self.edges.push(edge);
    }

    pub(super) fn provenance(
        &mut self,
        src: u32,
        range: Option<&octocode_engine::text::analysis::ObservedSpan>,
    ) -> ProvenanceRec {
        let file = &self.nodes[self.nodes[src as usize].file as usize];
        let path = &self.strings.strings[file.key as usize];
        let source_origin = self.sources.origin(path, range.copied());
        let digest = self.sources.digest(source_origin).unwrap_or_else(|| {
            self.built
                .facts
                .get(path)
                .map_or("", |facts| facts.source_digest.as_str())
        });
        ProvenanceRec {
            source_digest: self.strings.id(digest),
            source_start: range.map_or(NONE, |r| r.start_byte),
            source_end: range.map_or(NONE, |r| r.end_byte),
            source_origin,
            derivation_version: self.strings.id(&format!(
                "graph-analysis:{}/facts:{}",
                super::super::super::ANALYSIS_VERSION,
                octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION,
            )),
        }
    }

    /// Symbols, per file in source order; keys are `path#Qualified.name`
    /// with an `@line` suffix only when the qualified name repeats.
    pub(super) fn symbols(&mut self) -> Symbols<'a> {
        let mut symbols = Symbols::default();
        let built = self.built;
        for (file, facts) in &built.facts {
            let Some(&file_id) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            let index = self.file_symbols(file, facts, file_id, &mut symbols);
            symbols.indexes.insert(file.as_str(), index);
        }
        self.c_definitions(&mut symbols);
        symbols
    }

    pub(super) fn file_symbols(
        &mut self,
        file: &'a str,
        facts: &'a FileFacts,
        file_id: u32,
        symbols: &mut Symbols<'a>,
    ) -> FileIndex<'a> {
        let by_id = facts
            .declarations
            .iter()
            .map(|decl| (decl.id.as_str(), decl))
            .collect::<HashMap<_, _>>();
        let mut ordered = facts.declarations.iter().collect::<Vec<_>>();
        ordered.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.id.cmp(&b.id)));
        let keys = symbol_keys(file, &ordered, &by_id);
        let file_is_test = self.has_role(file, ROLE_TEST);
        let python = self.lang(file) == "python";
        let is_test_decl = |decl: &Declaration| {
            file_is_test
                || facts.test_scoped(decl.line)
                || (python
                    && ((matches!(decl.kind.as_str(), "function" | "method")
                        && decl.name.starts_with("test_"))
                        || (decl.kind == "class" && decl.name.starts_with("Test"))))
        };
        // A variable holding a `require`d module links through its binding;
        // it is never a call or type target of its own, yet it keeps its
        // name occupied as it did when it was one.
        let module_holders = facts
            .commonjs
            .iter()
            .map(|binding| binding.declaration_id.as_str())
            .collect::<HashSet<_>>();
        // Generated, bundled, vendored and declaration-only code never
        // serves as a unique-name call target.
        let unique_target = !self.has_role(file, ROLE_NOT_AUTHORED | ROLE_DECLARATION);
        let mut index = FileIndex {
            decl_nodes: HashMap::new(),
            by_name: HashMap::new(),
            exports: HashMap::new(),
        };
        for (decl, key) in ordered.iter().zip(&keys) {
            let id = self.nodes.len() as u32;
            let flags = if decl.exported { FLAG_EXPORTED } else { 0 }
                | if is_test_decl(decl) { FLAG_TEST } else { 0 }
                | if facts.reference_counts.get(&decl.id).is_some_and(|n| *n > 0) {
                    FLAG_LOCAL_USE
                } else {
                    0
                };
            self.nodes.push(NodeRec {
                kind: NodeKind::Symbol,
                flags,
                key: self.strings.id(key),
                name: self.strings.id(&decl.name),
                detail: self.strings.id(&decl.kind),
                file: file_id,
                parent: NONE,
                line: decl.line,
                end_line: decl.end_line,
            });
            index.decl_nodes.insert(decl.id.as_str(), id);
            symbols.targets.resize(id as usize + 1, 0);
            // Only a declaration whose own extent was recovered may carry a
            // repaired identity; one recovered through an enclosing
            // declaration (a malformed sibling in the same body) parsed
            // cleanly and stays a target.
            symbols.targets[id as usize] = if matches!(
                decl.parse_assessment,
                octocode_engine::graph::GraphParseAssessment::Recovered {
                    scope: octocode_engine::graph::GraphParseScope::Declaration
                        | octocode_engine::graph::GraphParseScope::Occurrence
                        | octocode_engine::graph::GraphParseScope::Document,
                    ..
                }
            ) {
                RECOVERED_TARGET
            } else if module_holders.contains(decl.id.as_str()) {
                REFERENCE_TARGET | RECOVERED_TARGET
            } else {
                target_bits(&decl.kind)
            };
            let source_origin = self.sources.origin(file, decl.source_range);
            let source_digest = self
                .sources
                .digest(source_origin)
                .unwrap_or(&facts.source_digest);
            self.declarations.push(DeclarationRec {
                test_identity: self
                    .strings
                    .id(&serde_json::json!(decl.test_identity).to_string()),
                node: id,
                occurrence: self.strings.id(&decl.id),
                source_digest: self.strings.id(source_digest),
                source_start: decl.source_range.as_ref().map_or(NONE, |r| r.start_byte),
                source_end: decl.source_range.as_ref().map_or(NONE, |r| r.end_byte),
                source_origin,
                selection_start: decl.selection_byte_range.map_or(NONE, |r| r.start_byte),
                selection_end: decl.selection_byte_range.map_or(NONE, |r| r.end_byte),
                selection_origin: self.sources.origin(file, decl.selection_byte_range),
                assessment: self
                    .strings
                    .id(&serde_json::json!(decl.parse_assessment).to_string()),
            });
            // A method declared outside its type's body (a Go receiver) is
            // no name of its module: only its type's method set reaches it.
            let owner = decl.owner.as_deref().filter(|_| decl.parent.is_none());
            if let Some(owner) = owner {
                symbols.container_names.insert(id, owner);
            } else {
                index
                    .by_name
                    .entry(decl.name.as_str())
                    .or_default()
                    .push(id);
            }
            if decl.exported && owner.is_none() {
                for public in decl.public_names() {
                    index.exports.entry(public.as_str()).or_default().push(id);
                }
            }
            if unique_target {
                symbols
                    .global_names
                    .entry(decl.name.as_str())
                    .or_default()
                    .push(id);
            }
        }
        for decl in &facts.declarations {
            if let (Some(parent), Some(&child)) =
                (&decl.parent, index.decl_nodes.get(decl.id.as_str()))
                && let Some(&parent_id) = index.decl_nodes.get(parent.as_str())
                && parent_id != child
            {
                self.nodes[child as usize].parent = parent_id;
                if let Some(parent_decl) = by_id.get(parent.as_str()) {
                    symbols
                        .container_names
                        .insert(child, parent_decl.name.as_str());
                }
            }
        }
        index
    }
}
