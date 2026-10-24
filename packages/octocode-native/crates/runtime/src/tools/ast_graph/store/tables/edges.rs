//! The edge families the projector adds between nodes: contains, imports,
//! uses and calls.
use super::*;
use octocode_engine::graph::FileGraphNode;

impl<'a> Projector<'a> {
    /// contains: file → top-level symbol, symbol → member.
    pub(super) fn contains_edges(&mut self) {
        let owned = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Symbol)
            .map(|(id, node)| {
                let owner = if node.parent == NONE {
                    node.file
                } else {
                    node.parent
                };
                (owner, id as u32, node.line)
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
        for (file, node) in &built.nodes {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            self.file_import_edges(node, src);
            let mut seen = BTreeMap::<u32, (u32, &'static str)>::new();
            for external in externals_by_file.get(file.as_str()).into_iter().flatten() {
                if let Some(&dst) = package_nodes.get(external.name.as_str()) {
                    let entry = seen
                        .entry(dst)
                        .or_insert((external.line, external.declared));
                    entry.0 = entry.0.min(external.line);
                    // A non-test import outranks a test-scoped one.
                    if entry.1 == "external-test" {
                        entry.1 = external.declared;
                    }
                }
            }
            for (dst, (line, declared)) in seen {
                self.push(
                    src,
                    dst,
                    EdgeKind::Imports,
                    Confidence::High,
                    declared,
                    line,
                );
            }
        }
    }

    pub(super) fn file_import_edges(&mut self, node: &FileGraphNode, src: u32) {
        for (target, kinds) in &node.edges {
            let Some(&dst) = self.file_nodes.get(target.as_str()) else {
                continue;
            };
            for (kind, &line) in kinds {
                let line = if line == 0 { NONE } else { line };
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
    }

    /// uses: named imports resolved (through re-exports) to the exported
    /// declaration; namespace imports mark the whole target file as used.
    pub(super) fn use_edges(&mut self, resolver: &ExportResolver) {
        let built = self.built;
        for (file, facts) in &built.facts {
            let Some(&src) = self.file_nodes.get(file.as_str()) else {
                continue;
            };
            for import in &facts.imports {
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
                let (found, hopped) = resolver.exported(target, name);
                let confidence = if hopped {
                    Confidence::Medium
                } else {
                    Confidence::High
                };
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

    /// calls and inherits: syntax call sites and base-type clauses linked to
    /// declarations.
    pub(super) fn call_edges(&mut self, resolver: &ExportResolver, symbols: &Symbols) -> CallStats {
        let mut file_paths = vec![""; self.nodes.len()];
        for (path, id) in &self.file_nodes {
            file_paths[*id as usize] = path;
        }
        let scope = link_scope(&file_paths, self.workspace);
        let members = member_index(&symbols.global_names, &symbols.container_names);
        let linker = Linker {
            resolver,
            nodes: &self.nodes,
            global_names: &symbols.global_names,
            members: &members,
            scope: &scope,
        };
        let accepts = |bit: u8| {
            move |id: u32| {
                symbols
                    .targets
                    .get(id as usize)
                    .is_some_and(|bits| bits & bit != 0)
            }
        };
        let callable = accepts(CALL_TARGET);
        let is_type = accepts(TYPE_TARGET);
        let mut calls = CallStats::default();
        let mut linked = Vec::<PendingEdge>::new();
        for (file, facts) in &self.built.facts {
            // Minified bundles re-declare everything under short names;
            // linking their call sites would only add noise.
            if self.has_role(file, ROLE_BUNDLED) {
                continue;
            }
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
            link_calls(&linker, &unit, facts, &callable, &mut calls, &mut linked);
            link_heritage(&linker, &unit, facts, &is_type, &mut linked);
        }
        for (src, dst, kind, confidence, detail, line) in linked {
            self.push(src, dst, kind, confidence, &detail, line);
        }
        calls
    }
}
