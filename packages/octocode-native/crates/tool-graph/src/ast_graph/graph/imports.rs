//! `FileLink`: edges, declarations, imports and package imports.
use super::*;
use crate::ast_graph::linking::LinkerGraph;

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

    /// What a non-relative specifier names (relative ones are `External`
    /// here: their path decides). Project code, failing to link, is an
    /// internal gap, not proof of a third-party package: a JS/TS alias,
    /// workspace package, `#`/`@/`/`~/` convention or a path a scanned file
    /// ends in; a Python module the scan, the directories above it or a local
    /// path dependency spell; a Rust crate a path dependency names, or any when
    /// no manifest says. Where the project places that code beyond the scan
    /// root, it is [`BareTarget::OutsideRoot`].
    pub(super) fn bare_target(&self, spec: &str) -> BareTarget {
        if spec.starts_with('.') || spec.starts_with('/') {
            return BareTarget::External;
        }
        match self.lang {
            Linker::JsTs => match self.inputs.ctx.bare_target_js(spec, &self.file) {
                BareTarget::External if self.inputs.js_modules.named_by(spec).next().is_some() => {
                    BareTarget::Internal
                }
                target => target,
            },
            Linker::Python => self.inputs.ctx.python_bare_target(spec),
            Linker::Rust => {
                let segments = spec
                    .trim_start_matches("::")
                    .trim_end_matches(';')
                    .split("::")
                    .collect::<Vec<_>>();
                let head = segments.first().copied().unwrap_or_default();
                // A `crate::` path into a module the scan does not hold, or
                // into the scan's own module where that module's file lies
                // above the scan, names code above the snapshot root, not a
                // broken import; so does a `super` chain that climbs to that
                // module.
                if self.inputs.cargo_crates.scan_is_submodule() {
                    if head == "crate" && spec.starts_with("crate") {
                        match self.inputs.cargo_crates.inside_scan(&segments[1..]) {
                            None => return BareTarget::OutsideRoot,
                            Some(rest) if self.names_root_module_item(rest) => {
                                return BareTarget::OutsideRoot;
                            }
                            Some(_) => {}
                        }
                    }
                    let climbs = segments.iter().take_while(|s| **s == "super").count();
                    let depth = rust_module_child_dir(&self.file)
                        .split('/')
                        .filter(|s| !s.is_empty() && *s != ".")
                        .count();
                    if climbs > 0
                        && climbs == depth
                        && self.names_root_module_item(&segments[climbs..])
                    {
                        return BareTarget::OutsideRoot;
                    }
                }
                self.inputs.cargo_crates.local_crate(&self.file, head)
            }
            _ => BareTarget::External,
        }
    }

    /// Whether `rest`, a path from the scan root's own module, names an
    /// item of that module's file where that file lies above the scan (no
    /// `mod.rs` at the scan root): anything but a module the scan holds.
    fn names_root_module_item(&self, rest: &[&str]) -> bool {
        let known = self.inputs.known;
        !known.contains("mod.rs")
            && rest.first().is_none_or(|module| {
                !known.contains(&format!("{module}.rs"))
                    && !known.contains(&format!("{module}/mod.rs"))
            })
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
        let bare = if target.is_none() {
            self.bare_target(spec)
        } else {
            BareTarget::External
        };
        record_resolution(
            self.b,
            &self.file,
            line,
            spec,
            self.lang,
            target,
            unsupported,
            bare,
            self.inputs.security,
        )
    }

    pub(super) fn declarations(
        &mut self,
        edges: &[octocode_engine::graph::GraphFactEdge],
        declarations: Vec<octocode_engine::graph::GraphFactDeclaration>,
    ) {
        for edge in edges {
            if matches!(edge.relation.as_str(), "extends" | "implements" | "embeds") {
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
                end_line: d.range.end.line + 1,
                exported: d.exported,
                exported_as: d.exported_as,
                parent: d.parent,
                owner: d.owner,
                prototype: d.prototype,
                return_type: d.return_type,
                try_type: d.try_type,
                return_elements: d.return_elements,
                asynchronous: d.asynchronous,
                function_typed: d.function_typed,
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
        let inline = rust
            && i.module_scope
                .as_ref()
                .is_some_and(|scope| !scope.is_empty());
        // Rust 2018 paths start at any name the module binds: a `use` path
        // whose first segment an earlier `use` binds continues that one's
        // path (`use scanned::streamed;` then `use streamed::walk_cut;`).
        let mut i = i;
        if rust && let Some(specifier) = self.rust_rebound(&i) {
            i.specifier = specifier;
        }
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
        } else {
            self.resolve(
                &rust_scoped_specifier(&i, self.lang),
                i.resolution_hint.as_deref(),
                i.imported_name.as_deref().unwrap_or_default(),
            )
        };
        let unsupported = self.lang == Linker::Unsupported;
        // An unresolved include names a file outside the scan, never a package;
        // a quoted C include is one the project's search path missed.
        let quoted =
            self.lang == Linker::CFamily && i.resolution_hint.as_deref() == Some("c-relative");
        let external = if include || quoted {
            record_resolution(
                self.b,
                &self.file,
                i.line,
                &i.specifier,
                self.lang,
                &target,
                unsupported,
                BareTarget::Internal,
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
            // A binding read only as a type: the emit settings decide its load.
            if i.type_use_only {
                kind = super::super::edges::TYPE_USE_IMPORT;
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
            scope_id: i.scope_id,
            inline,
        });
    }

    /// The path of a file-level Rust `use` whose first segment another
    /// file-level `use` of this file binds to linked code, with that segment
    /// replaced by the binding's own path; `None` for a path that starts at
    /// a keyword, a child module or a crate.
    fn rust_rebound(&self, i: &octocode_engine::graph::GraphFactImport) -> Option<String> {
        if i.module_scope
            .as_ref()
            .is_some_and(|scope| !scope.is_empty())
        {
            return None;
        }
        let head = i.specifier.split("::").next()?;
        let rest = i.specifier.strip_prefix(head)?;
        if matches!(head, "" | "crate" | "self" | "super") || rest.is_empty() {
            return None;
        }
        let bound = self.facts.imports.iter().find(|import| {
            !import.inline
                && import.target.is_some()
                && import.imported_name != "*"
                && import
                    .local_name
                    .as_deref()
                    .unwrap_or(&import.imported_name)
                    == head
        })?;
        Some(format!("{}{rest}", bound.specifier))
    }

    /// Go and Java imports name packages: one import fact per linked file,
    /// so every package edge keeps its import line (`importLine`).
    pub(super) fn package_import(&mut self, i: octocode_engine::graph::GraphFactImport) {
        let mut external = false;
        let targets = match self
            .inputs
            .packages
            .resolve(self.lang, &i.specifier, &self.file)
        {
            PackageLink::Files(files) => files,
            PackageLink::Unlinked(BareTarget::External) => {
                self.b
                    .count_import(ImportOutcome::External, &self.file, i.line, &i.specifier);
                external = true;
                Vec::new()
            }
            PackageLink::Unlinked(target) => {
                record_unresolved_internal(
                    self.b,
                    &self.file,
                    i.line,
                    &i.specifier,
                    target == BareTarget::OutsideRoot,
                    self.inputs.security,
                );
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
            }
            linked_targets.push(target);
        }
        if linked_targets.len() > 1 {
            self.b
                .namespace_targets
                .extend(linked_targets.iter().cloned());
        }
        if linked_targets.is_empty() {
            self.facts.imports.push(Import {
                id: i.id.clone(),
                imported_name: i.imported_name.unwrap_or_default(),
                local_name: i.local_name,
                specifier: i.specifier,
                line: i.line,
                target: None,
                external,
                scope_id: i.scope_id,
                inline: false,
            });
            return;
        }
        // `import static a.b.C.member;` binds `member` of `C`'s file;
        // `import static a.b.C.*;` binds all of them, like a glob.
        let (imported_name, local_name) = match java_static_member(&i.specifier)
            .filter(|_| self.lang == Linker::Java && linked_targets.len() == 1)
        {
            Some(member) => (
                member.to_owned(),
                (member != "*").then(|| member.to_owned()),
            ),
            None => (i.imported_name.unwrap_or_default(), i.local_name),
        };
        let packages = self.inputs.packages;
        for target in linked_targets {
            // A plain Go import binds the name the package's clause gives it,
            // which need not be the path's last segment.
            let local_name = local_name.clone().or_else(|| {
                packages
                    .go_package_name(&target)
                    .filter(|_| self.lang == Linker::Go)
                    .map(str::to_owned)
            });
            self.facts.imports.push(Import {
                id: i.id.clone(),
                imported_name: imported_name.clone(),
                local_name,
                specifier: i.specifier.clone(),
                line: i.line,
                target: Some(target),
                external: false,
                scope_id: i.scope_id.clone(),
                inline: false,
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

/// The member a Java `import static a.b.C.member;` names (`*` for a static
/// glob); `None` for a type or package import.
fn java_static_member(specifier: &str) -> Option<&str> {
    let (_, member) = specifier.strip_prefix("static ")?.trim().rsplit_once('.')?;
    (!member.is_empty()).then_some(member)
}
