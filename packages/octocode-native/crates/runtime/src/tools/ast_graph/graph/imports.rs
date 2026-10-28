//! `FileLink`: edges, declarations, imports and package imports.
use super::*;

impl FileLink<'_, '_> {
    pub(super) fn edge(&mut self, target: &str, kind: &str, line: u32) {
        let facts = &self.facts;
        add_edge(self.b, &mut self.node, target, kind, line, &|line| {
            facts.test_scoped(line)
        });
        if self
            .node
            .edges
            .get(target)
            .is_some_and(|kinds| kinds.contains_key(kind))
        {
            self.facts
                .file_edges
                .push((target.to_owned(), kind.to_owned(), line));
        }
    }

    /// Whether `line` lies inside a function or method body of this file
    /// (after its header line): code there runs on call.
    pub(super) fn inside_function(&self, line: u32) -> bool {
        self.facts.declarations.iter().any(|d| {
            matches!(d.kind.as_str(), "function" | "method") && d.line < line && line <= d.end_line
        })
    }

    /// A bare JS/TS specifier naming project code (alias, workspace package,
    /// `#`/`@/`/`~/` convention): failing to link it is an internal gap.
    pub(super) fn internal_bare(&self, spec: &str) -> bool {
        self.lang == Linker::JsTs
            && !spec.starts_with('.')
            && !spec.starts_with('/')
            && self.inputs.ctx.is_internal_bare_js(spec, &self.file)
    }

    pub(super) fn resolve(&self, spec: &str, hint: Option<&str>, imported: &str) -> Option<String> {
        if self.lang == Linker::Rust {
            return self.inputs.rust_modules.resolve(
                spec,
                &self.file,
                self.inputs.known,
                self.inputs.cargo_crates,
            );
        }
        resolve(
            spec,
            &self.file,
            self.lang,
            hint,
            imported,
            self.inputs.known,
            self.inputs.cargo_crates,
            self.inputs.ctx,
        )
    }

    pub(super) fn record(
        &mut self,
        line: u32,
        spec: &str,
        target: &Option<String>,
        unsupported: bool,
    ) -> bool {
        let internal = target.is_none() && self.internal_bare(spec);
        record_resolution(
            self.b,
            &self.file,
            line,
            spec,
            self.lang,
            target,
            unsupported,
            internal,
            self.inputs.security,
        )
    }

    pub(super) fn declarations(
        &mut self,
        edges: &[octocode_engine::graph::GraphFactEdge],
        declarations: Vec<octocode_engine::graph::GraphFactDeclaration>,
    ) {
        for edge in edges {
            if matches!(edge.relation.as_str(), "extends" | "implements") {
                self.facts.heritage.push(Heritage {
                    decl_id: edge.from.clone(),
                    relation: edge.relation.clone(),
                    target: edge.to.clone(),
                    line: edge.line,
                });
            }
        }
        for d in declarations {
            self.facts.declarations.push(Declaration {
                id: d.id,
                test_identity: d.test_identity,
                source_range: d.source_range,
                selection_byte_range: d.selection_byte_range,
                parse_assessment: d.parse_assessment,
                name: d.name,
                kind: d.kind,
                line: d.line,
                end_line: d.line + d.range.end.line.saturating_sub(d.range.start.line),
                exported: d.exported,
                exported_as: d.exported_as,
                parent: d.parent,
            });
        }
    }

    pub(super) fn import(
        &mut self,
        i: octocode_engine::graph::GraphFactImport,
        modules: &[octocode_engine::graph::GraphFactRustModule],
    ) {
        if matches!(self.lang, Linker::Go | Linker::Java) {
            return self.package_import(i);
        }
        let rust = self.lang == Linker::Rust;
        // `include!("path")` splices that file into this one: the path is
        // relative to this file and its items share this file's namespace.
        // `include_str!`/`include_bytes!` resolve the same way but embed
        // data, so they share no names.
        let splice = rust && i.import_kind == "include";
        let include = splice || rust && i.import_kind == "include-data";
        let target = if include {
            join_within_root(dirname(&self.file), &i.specifier)
                .filter(|target| self.inputs.known.contains(target))
        } else if rust
            && !self.inputs.rust_cargo_unavailable
            && let ModuleResolution::Inside(target) =
                self.inputs
                    .rust_modules
                    .import(&self.file, &i, self.inputs.cargo_crates)
        {
            target
        } else if rust
            && let Some(local) = rust_local_target(&i, &self.file, modules, self.inputs.known)
        {
            Some(local)
        } else if rust && self.inputs.rust_cargo_unavailable {
            None
        } else {
            self.resolve(
                &rust_scoped_specifier(&i, self.lang),
                i.resolution_hint.as_deref(),
                i.imported_name.as_deref().unwrap_or_default(),
            )
        };
        let unsupported = self.lang == Linker::Unsupported
            || rust && !include && self.inputs.rust_cargo_unavailable;
        // An unresolved include names a file outside the scan, never a package.
        let external = if include {
            record_resolution(
                self.b,
                &self.file,
                i.line,
                &i.specifier,
                self.lang,
                &target,
                unsupported,
                true,
                self.inputs.security,
            )
        } else {
            self.record(i.line, &i.specifier, &target, unsupported)
        };
        if let Some(t) = target.as_ref().filter(|t| **t != self.file) {
            let kind = if i.import_kind.is_empty() {
                "value"
            } else {
                &i.import_kind
            };
            let mut kind = self.lang.import_edge(kind);
            if kind == super::super::edges::PYTHON_IMPORT && self.inside_function(i.line) {
                kind = super::super::edges::PYTHON_LAZY_IMPORT;
            }
            self.edge(t, kind, i.line);
            if i.imported_name.as_deref() == Some("*") || self.lang == Linker::CFamily || splice {
                self.b.namespace_targets.insert(t.clone());
            }
        }
        self.facts.imports.push(Import {
            id: i.id.clone(),
            imported_name: i.imported_name.unwrap_or_default(),
            local_name: i.local_name,
            specifier: i.specifier,
            line: i.line,
            target,
            external,
        });
    }

    /// Go and Java imports name packages: one import fact per linked file,
    /// so every package edge keeps its import line (`importLine`).
    pub(super) fn package_import(&mut self, i: octocode_engine::graph::GraphFactImport) {
        if self.b.edges_capped {
            return;
        }
        let mut external = false;
        let targets = match self
            .inputs
            .packages
            .resolve(self.lang, &i.specifier, &self.file)
        {
            PackageLink::Files(files) => files,
            PackageLink::UnresolvedInternal => {
                self.b.count_import(
                    ImportOutcome::UnresolvedInternal,
                    &self.file,
                    i.line,
                    &i.specifier,
                );
                self.b.diagnostics.push(Diagnostic::new(
                    self.file.clone(),
                    Some(i.line),
                    DiagnosticCode::UnresolvedInternal,
                    sanitize(
                        self.inputs.security,
                        &format!("Cannot link import {:?} (unresolvedInternal).", i.specifier),
                    ),
                ));
                Vec::new()
            }
            PackageLink::External => {
                self.b
                    .count_import(ImportOutcome::External, &self.file, i.line, &i.specifier);
                external = true;
                Vec::new()
            }
        };
        if !targets.is_empty() {
            self.b
                .count_import(ImportOutcome::Resolved, &self.file, i.line, &i.specifier);
        }
        let mut linked_targets = Vec::new();
        for target in targets {
            if target != self.file {
                self.edge(&target, self.lang.import_edge("value"), i.line);
                if self.b.edges_capped {
                    break;
                }
            }
            linked_targets.push(target);
        }
        if linked_targets.len() > 1 {
            self.b
                .namespace_targets
                .extend(linked_targets.iter().cloned());
        }
        let imported_name = i.imported_name.unwrap_or_default();
        if linked_targets.is_empty() {
            self.facts.imports.push(Import {
                id: i.id.clone(),
                imported_name,
                local_name: i.local_name,
                specifier: i.specifier,
                line: i.line,
                target: None,
                external,
            });
            return;
        }
        for target in linked_targets {
            self.facts.imports.push(Import {
                id: i.id.clone(),
                imported_name: imported_name.clone(),
                local_name: i.local_name.clone(),
                specifier: i.specifier.clone(),
                line: i.line,
                target: Some(target),
                external: false,
            });
        }
    }

    /// Same-package Java classes need no import: link the classes a call
    /// receiver, constructor, or heritage clause names.
    pub(super) fn java_same_package(
        &mut self,
        calls: &[octocode_engine::graph::GraphFactCall],
        edges: &[octocode_engine::graph::GraphFactEdge],
    ) {
        let mut named = BTreeMap::<&str, u32>::new();
        let uses = calls
            .iter()
            .flat_map(|call| {
                let head = call
                    .callee
                    .split(['.', '(', '<'])
                    .next()
                    .unwrap_or_default();
                [Some(head), call.receiver_type.as_deref()]
                    .into_iter()
                    .flatten()
                    .map(|name| (name, call.line))
            })
            .chain(
                edges
                    .iter()
                    .filter(|edge| matches!(edge.relation.as_str(), "extends" | "implements"))
                    .map(|edge| {
                        (
                            edge.to.split(['.', '<']).next().unwrap_or_default(),
                            edge.line,
                        )
                    }),
            );
        for (name, line) in uses {
            let first = named.entry(name).or_insert(line);
            *first = (*first).min(line);
        }
        let packages = self.inputs.packages;
        for (name, line) in named {
            if let Some(target) = packages.same_package_class(&self.file, name) {
                // Not an import: no import statement exists, so the edge
                // names its own kind and cites the first use.
                self.edge(target, "java-same-package", line);
            }
        }
    }
}
