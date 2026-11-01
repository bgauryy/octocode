//! The edge families the projector adds between nodes: contains, imports,
//! uses and calls.
use super::*;

impl<'a> Projector<'a> {
    /// contains: file → top-level symbol, symbol → member.
    pub(super) fn contains_edges(&mut self) {
        let owned = (0..=u32::MAX)
            .zip(&self.nodes)
            .filter(|(_, node)| node.kind == NodeKind::Symbol)
            .map(|(id, node)| {
                let owner = if node.parent == NONE {
                    node.file
                } else {
                    node.parent
                };
                (owner, id, node.line)
            })
            .collect::<Vec<_>>();
        for (owner, id, line) in owned {
            self.push(
                owner,
                id,
                EdgeKind::Contains,
                Confidence::High,
                "declares",
                line,
            );
        }
    }

    /// Relative imports that land on an unparsed file: linked instead of
    /// reported as a resolution failure.
    pub(super) fn unparsed_links(&self) -> UnparsedLinks<'a> {
        let mut links = Vec::new();
        let mut relinked = BTreeSet::new();
        if self.unparsed.is_empty() {
            return (links, relinked);
        }
        for (file, facts) in &self.built.facts {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            for import in facts
                .imports
                .iter()
                .filter(|i| i.target.is_none() && !i.external)
            {
                if let Some(target) = unparsed_target(file, &import.specifier, &self.unparsed)
                    && let Some(&dst) = self.file_nodes.get(target)
                {
                    links.push((src, dst, import.line));
                    relinked.insert((file.as_str(), import.line));
                }
            }
        }
        (links, relinked)
    }

    /// imports: linked file edges (one per relation) and external packages.
    pub(super) fn import_edges(
        &mut self,
        externals: &[External],
        package_nodes: &BTreeMap<String, u32>,
    ) {
        let mut externals_by_file = HashMap::<&str, Vec<&External>>::new();
        for external in externals {
            externals_by_file
                .entry(external.file)
                .or_default()
                .push(external);
        }
        let built = self.built;
        for file in built.nodes.keys() {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            if let Some(facts) = built.facts.get(file) {
                self.file_import_edges(&facts.file_edges, src);
            }
            for external in externals_by_file.get(file.as_str()).into_iter().flatten() {
                if let Some(&dst) = package_nodes.get(external.name.as_str()) {
                    self.push(
                        src,
                        dst,
                        EdgeKind::Imports,
                        Confidence::High,
                        external.declared,
                        external.line,
                    );
                }
            }
        }
    }

    pub(super) fn file_import_edges(&mut self, edges: &[(String, String, u32)], src: u32) {
        for (target, kind, line) in edges {
            let Some(&dst) = self.file_nodes.get(target.as_str()) else {
                continue;
            };
            let line = if *line == 0 { NONE } else { *line };
            // `mod child;` declares a submodule: containment, not a
            // dependency (it would fold every module tree into a cycle).
            let (edge_kind, detail) =
                if crate::tools::ast_graph::edges::load(kind) == Load::ModuleTree {
                    (EdgeKind::Contains, MODULE_DETAIL)
                } else {
                    (EdgeKind::Imports, kind.as_str())
                };
            self.push(src, dst, edge_kind, Confidence::High, detail, line);
        }
    }

    /// uses: named imports resolved (through re-exports) to the exported
    /// declaration; namespace imports mark the whole target file as used.
    pub(super) fn use_edges(&mut self, resolver: &ExportResolver) {
        let built = self.built;
        for (file, facts) in &built.facts {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            let commonjs = facts.commonjs.iter().map(|binding| &binding.import);
            for import in facts.imports.iter().chain(commonjs) {
                let Some(target) = &import.target else {
                    continue;
                };
                let name = import.imported_name.as_str();
                if name.is_empty() {
                    continue;
                }
                if name == "*" {
                    if let Some(&dst) = self.file_nodes.get(target.as_str()) {
                        self.push(
                            src,
                            dst,
                            EdgeKind::Uses,
                            Confidence::High,
                            "namespace",
                            import.line,
                        );
                    }
                    continue;
                }
                let (mut found, mut hopped) = resolver.exported(target, name);
                // Node interop, as calls resolve it: an ESM default import of
                // a CommonJS module uses its `module.exports` value.
                if name == "default" && found.is_empty() {
                    (found, hopped) =
                        resolver.exported(target, octocode_engine::graph::COMMONJS_WHOLE_EXPORT);
                }
                let confidence = confidence_for(&found, Confidence::High);
                for dst in found {
                    self.push(
                        src,
                        dst,
                        EdgeKind::Uses,
                        confidence,
                        if hopped { "reexport" } else { "import" },
                        import.line,
                    );
                }
            }
        }
    }

    /// The names Rust files bind repo macros to: each `macro_rules!` name,
    /// and the local name of a `use` or `pub use` of one (`use my_ready as
    /// ready`), which a glob import carries to other files.
    fn rust_macro_names(&self) -> HashSet<&str> {
        let rust = || {
            self.built
                .facts
                .iter()
                .filter(|(file, _)| self.lang(file) == "rust")
                .map(|(_, facts)| facts)
        };
        let declared = rust()
            .flat_map(|facts| &facts.declarations)
            .filter(|declaration| declaration.kind == "macro")
            .map(|declaration| declaration.name.as_str())
            .collect::<HashSet<_>>();
        let aliases = rust().flat_map(|facts| {
            let imports = facts
                .imports
                .iter()
                .filter(|import| declared.contains(import.imported_name.as_str()))
                .filter_map(|import| import.local_name.as_deref());
            let reexports = facts
                .reexports
                .iter()
                .filter(|reexport| declared.contains(reexport.imported_name.as_str()))
                .map(|reexport| reexport.local_name.as_str());
            imports.chain(reexports)
        });
        aliases.chain(declared.iter().copied()).collect()
    }

    /// calls and inherits: syntax call sites and base-type clauses linked to
    /// declarations.
    /// Each Rust function's declared return types by declaration node (see
    /// [`Linker::return_types`]).
    fn return_types<'s>(&'s self, symbols: &Symbols) -> HashMap<u32, ReturnTypes<'s>> {
        let mut return_types = HashMap::new();
        for (file, facts) in &self.built.facts {
            let Some(index) = symbols.indexes.get(file.as_str()) else {
                continue;
            };
            for decl in &facts.declarations {
                if let Some(&id) = index.decl_nodes.get(decl.id.as_str())
                    && (decl.return_type.is_some() || decl.try_type.is_some())
                {
                    return_types
                        .insert(id, (decl.return_type.as_deref(), decl.try_type.as_deref()));
                }
            }
        }
        return_types
    }

    pub(super) fn call_edges(&mut self, resolver: &ExportResolver, symbols: &Symbols) -> CallStats {
        let mut file_paths = vec![""; self.nodes.len()];
        let mut families = vec![""; self.nodes.len()];
        for (path, id) in &self.file_nodes {
            file_paths[*id as usize] = path;
            families[*id as usize] = link_family(self.lang(path));
        }
        let scope = link_scope(&file_paths, families, self.workspace);
        let members = member_index(&symbols.global_names, &symbols.container_names);
        let accepts = |bit: u8| {
            move |id: u32| {
                symbols
                    .targets
                    .get(id as usize)
                    .is_some_and(|bits| bits & bit != 0)
            }
        };
        let callable = accepts(CALL_TARGET);
        let method = accepts(METHOD_TARGET);
        let is_type = accepts(TYPE_TARGET);
        let recovered = accepts(RECOVERED_TARGET);
        let empty_bases = BaseIndex::default();
        let rust_macros = self.rust_macro_names();
        let return_types = self.return_types(symbols);
        let linker = Linker {
            resolver,
            nodes: &self.nodes,
            strings: &self.strings.strings,
            global_names: &symbols.global_names,
            members: &members,
            scope: &scope,
            bases: &empty_bases,
            is_type: &is_type,
            recovered: &recovered,
            rust_macros: &rust_macros,
            return_types: &return_types,
        };
        let mut calls = CallStats::default();
        for file in self.built.facts.keys() {
            calls
                .by_language
                .entry(self.lang(file).to_owned())
                .or_default();
        }
        let mut linked = Vec::<PendingEdge>::new();
        let mut unresolved_bases = HashSet::new();
        let mut scala_bases = HashMap::new();
        let mut units = Vec::new();
        for (file, facts) in &self.built.facts {
            let (Some(index), Some(&file_id)) = (
                symbols.indexes.get(file.as_str()),
                self.file_nodes.get(file.as_str()),
            ) else {
                continue;
            };
            let language = self.lang(file);
            let unit = FileLinks {
                bindings: FileBindings::new(facts, language),
                index,
                file_id,
                language,
            };
            unresolved_bases.extend(link_heritage(
                &linker,
                &unit,
                facts,
                &is_type,
                &mut linked,
                &mut scala_bases,
            ));
            units.push((unit, facts));
        }
        let bases = base_index(&linked, unresolved_bases, scala_bases);
        let linker = Linker {
            bases: &bases,
            ..linker
        };
        let mut call_output = CallOutput::default();
        for (unit, facts) in &units {
            link_calls(
                &linker,
                unit,
                facts,
                &callable,
                &method,
                &mut calls,
                &mut call_output,
            );
        }
        let reference = accepts(REFERENCE_TARGET);
        let lexical_scope = accepts(LEXICAL_SCOPE);
        let empty_index = FileIndex {
            decl_nodes: HashMap::new(),
            by_name: HashMap::new(),
            exports: HashMap::new(),
        };
        let mut arguments = Vec::new();
        for (file, facts) in &self.built.facts {
            let Some(&file_id) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            let language = self.lang(file);
            let unit = FileLinks {
                bindings: FileBindings::new(facts, language),
                index: symbols.indexes.get(file.as_str()).unwrap_or(&empty_index),
                file_id,
                language,
            };
            arguments::link_arguments(
                &linker,
                &unit,
                facts,
                &reference,
                &lexical_scope,
                &mut arguments,
            );
        }
        for (src, dst, kind, confidence, detail, line) in linked {
            self.push(src, dst, kind, confidence, &detail, line);
        }
        self.push_calls(symbols, call_output);
        self.push_arguments(arguments);
        calls
    }

    /// Writes linked call edges (with their witnesses) and unresolved call sites.
    fn push_calls(&mut self, symbols: &Symbols, call_output: CallOutput) {
        let mut redirected = HashSet::new();
        for call in call_output.edges {
            let (src, dst, kind, confidence, detail, line) = call.edge;
            // A C-family prototype stands for its one definition; a site
            // (occurrence ids repeat across files) links it once.
            let dst = symbols.definitions.get(&dst).copied().unwrap_or(dst);
            if !symbols.definitions.is_empty()
                && !redirected.insert((src, call.occurrence.clone(), dst))
            {
                continue;
            }
            self.push(src, dst, kind, confidence, &detail, line);
            let occurrence = self.strings.id(&call.occurrence);
            let provenance = self.provenance(src, call.source_range.as_ref());
            if let Some(witness) = self.witnesses.last_mut() {
                witness.occurrence = occurrence;
                witness.provenance = provenance;
            }
        }
        for call in call_output.unresolved {
            let provenance = self.provenance(call.src, call.source_range.as_ref());
            self.unresolved.push(UnresolvedRec {
                argument: NONE,
                provenance,
                relation: EdgeKind::Calls,
                src: call.src,
                occurrence: self.strings.id(&call.occurrence),
                callee: self.strings.id(&call.callee),
                line: call.line,
                reason: self.strings.id(call.reason),
            });
        }
    }
}
