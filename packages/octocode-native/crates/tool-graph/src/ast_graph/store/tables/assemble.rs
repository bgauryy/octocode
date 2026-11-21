//! External package nodes and assembling the final projection.
use super::*;

impl<'a> Projector<'a> {
    /// External packages. Each unlinked external import is classified
    /// against its component manifest once; a Cargo root no manifest
    /// declares is a local module (Cargo would not build it otherwise), so
    /// it gets no node.
    pub(super) fn externals(&mut self) -> (Vec<External<'a>>, BTreeMap<String, u32>) {
        let mut externals = Vec::<External>::new();
        let mut packages = BTreeMap::<String, &'static str>::new();
        let built = self.built;
        for (file, facts) in &built.facts {
            let language = self.lang(file);
            let eco = ecosystem(language);
            for import in facts
                .imports
                .iter()
                .filter(|i| i.external && i.target.is_none())
            {
                let name = if eco == "go" {
                    self.workspace
                        .go_module(&import.specifier)
                        .map(str::to_owned)
                        .or_else(|| package_name(&import.specifier, language))
                } else {
                    package_name(&import.specifier, language)
                };
                let Some(name) = name else { continue };
                let mut declared = self.workspace.declared(file, &name, eco);
                if eco == "cargo" && declared == Declared::Unknown {
                    continue;
                }
                if declared != Declared::Builtin && facts.test_scoped(import.line) {
                    declared = Declared::TestScoped;
                }
                packages.entry(name.clone()).or_insert(eco);
                externals.push(External {
                    file: file.as_str(),
                    name,
                    declared: declared.as_str(),
                    line: import.line,
                });
            }
        }
        let mut package_nodes = BTreeMap::<String, u32>::new();
        for (name, eco) in packages {
            let id = octocode_engine::num::saturating_u32(self.nodes.len());
            self.nodes.push(NodeRec {
                kind: NodeKind::Package,
                flags: 0,
                key: self.strings.id(&format!("pkg:{name}")),
                name: self.strings.id(&name),
                detail: self.strings.id(eco),
                file: NONE,
                parent: NONE,
                line: NONE,
                end_line: NONE,
            });
            package_nodes.insert(name, id);
        }
        (externals, package_nodes)
    }

    /// One edge per (src, kind, dst, detail): the earliest line wins, so a
    /// callee invoked ten times from one caller is one edge.
    pub(super) fn finish(
        mut self,
        relinked: &BTreeSet<(&str, u32)>,
        calls: CallStats,
    ) -> Projection {
        // Production evidence outranks test evidence, then the earliest line.
        self.edges.sort_by(|a, b| {
            (a.src, a.kind, a.dst, a.detail, a.test, a.line, a.confidence).cmp(&(
                b.src,
                b.kind,
                b.dst,
                b.detail,
                b.test,
                b.line,
                b.confidence,
            ))
        });
        self.edges.dedup_by(|b, a| {
            a.src == b.src && a.kind == b.kind && a.dst == b.dst && a.detail == b.detail
        });
        // Between two nodes, the earliest production evidence comes first: a
        // walk that shows one edge per neighbor cites the file's first
        // production reference, and a test reference only when it is all.
        self.edges
            .sort_by_key(|e| (e.src, e.kind, e.dst, e.test, e.line, e.detail, e.confidence));
        let built = self.built;
        // An import relinked to an unparsed file is resolved: it leaves the
        // unresolved diagnostics and the unresolved tally together, so both
        // answers share this one decision.
        let relinked_diag = |diag: &crate::ast_graph::types::Diagnostic| {
            diag.code == DiagnosticCode::UnresolvedInternal
                && diag
                    .line
                    .is_some_and(|line| relinked.contains(&(diag.file.as_str(), line)))
        };
        let mut imports = built.imports;
        let relinked_statements = built
            .diagnostics
            .iter()
            .filter(|diag| relinked_diag(diag))
            .map(|diag| (diag.file.as_str(), diag.line, diag.message.as_str()))
            .collect::<BTreeSet<_>>()
            .len();
        let relinked_statements = octocode_engine::num::saturating_u32(relinked_statements);
        let unresolved = &mut imports[ImportOutcome::UnresolvedInternal as usize];
        let moved = relinked_statements.min(*unresolved);
        *unresolved -= moved;
        imports[ImportOutcome::Resolved as usize] += moved;
        let mut diagnostics = built
            .diagnostics
            .iter()
            .filter(|diag| !relinked_diag(diag))
            .map(|diag| DiagRec {
                file: self.strings.id(&diag.file),
                line: diag.line.unwrap_or(NONE),
                code: self.strings.id(diag.code.as_str()),
                message: self.strings.id(&diag.message),
                target: diag
                    .target
                    .as_deref()
                    .and_then(|target| self.file_nodes.get(target).copied())
                    .unwrap_or(NONE),
            })
            .collect::<Vec<_>>();
        diagnostics.dedup();
        let digests = self
            .file_nodes
            .iter()
            .filter_map(|(file, id)| {
                let digest = &built.facts.get(*file)?.digest;
                (!digest.is_empty()).then(|| (*id, self.strings.id(digest)))
            })
            .collect();
        let stamps = self
            .file_nodes
            .iter()
            .filter_map(|(file, id)| {
                let facts = built.facts.get(*file)?;
                let stamp = facts.stamp.filter(|_| !facts.digest.is_empty())?;
                Some((*id, stamp))
            })
            .collect();
        let components = self
            .file_nodes
            .iter()
            .filter_map(|(file, id)| {
                let language = self.languages.get(file).map_or("", String::as_str);
                let component = self.workspace.component_of(file, ecosystem(language))?;
                let dir = if component.dir.is_empty() {
                    "."
                } else {
                    component.dir.as_str()
                };
                Some((
                    *id,
                    self.strings.id(dir),
                    self.strings.id(&component.name),
                    self.strings.id(&component.meta()),
                ))
            })
            .collect();
        // A file a production file imports is that file's module: an entry
        // rule that holds only for files run on their own drops it.
        let imported = self
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Imports && !edge.test && edge.src != edge.dst)
            .map(|edge| edge.dst)
            .collect::<HashSet<_>>();
        let workspace = self.workspace;
        let mut entries = Vec::new();
        for (file, rule) in &workspace.entries {
            let Some(&node) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            if super::super::workspace::unless_imported(rule) && imported.contains(&node) {
                self.nodes[node as usize].flags &= !ROLE_ENTRY;
                if let Some(roles) = self.roles.get_mut(file.as_str()) {
                    *roles &= !ROLE_ENTRY;
                }
                continue;
            }
            entries.push((node, self.strings.id(rule)));
        }
        let mut tables = GraphTables {
            sources: self.sources.table,
            components,
            entries,
            strings: self.strings.strings.into(),
            nodes: self.nodes,
            edges: self.edges,
            witnesses: self.witnesses,
            unresolved: self.unresolved,
            declarations: self.declarations,
            diagnostics,
            digests,
            stamps,
            ..Default::default()
        };
        tables.index();
        let unextracted = super::arguments::declared_unextracted(self.unextracted, &tables);
        Projection {
            tables,
            calls,
            imports,
            unextracted,
        }
    }
}
