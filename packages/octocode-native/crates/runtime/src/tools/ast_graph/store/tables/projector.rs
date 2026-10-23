//! Building the projector: file nodes and their symbol nodes.
use super::*;

impl<'a> Projector<'a> {
    /// File nodes, sorted by path.
    pub(super) fn new(built: &'a BuiltGraph, workspace: &'a Workspace) -> Self {
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
            built,
            workspace,
            strings: Interner::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
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
        self.edges.push(EdgeRec {
            src,
            dst,
            kind,
            confidence,
            detail,
            line,
            test,
        });
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
            // Minified bundles declare thousands of mangled names: keep the
            // file and its imports, drop the symbols.
            if self.has_role(file, ROLE_BUNDLED) {
                continue;
            }
            let index = self.file_symbols(file, facts, file_id, &mut symbols);
            symbols.indexes.insert(file.as_str(), index);
        }
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
            symbols.targets[id as usize] = target_bits(&decl.kind);
            index
                .by_name
                .entry(decl.name.as_str())
                .or_default()
                .push(id);
            if decl.exported {
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
