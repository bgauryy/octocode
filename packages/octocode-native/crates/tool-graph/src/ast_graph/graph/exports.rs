//! `FileLink`: exports, calls, path specifiers and CommonJS requires.
use super::super::edges::{RECOVERED_NAMED_REEXPORT, RECOVERED_STAR_REEXPORT};
use super::*;
use octocode_engine::graph::{COMMONJS_WHOLE_EXPORT, UNKNOWN_LOAD};

impl FileLink<'_, '_> {
    pub(super) fn export(&mut self, x: octocode_engine::graph::GraphFactExport) {
        let Some(spec) = x.source else {
            self.export_import_binding(&x);
            return;
        };
        let target = self.resolve(&spec, None, x.local_name.as_deref().unwrap_or(&x.name));
        let kind = self.reexport_kind(&x.name, &x.export_kind);
        let external = self.record(
            x.line,
            &spec,
            &target,
            self.lang == Linker::Unsupported,
            kind,
        );
        self.package_load(external, &spec, x.line, kind);
        if let Some(t) = &target {
            self.facts.reexport_lines.push((t.clone(), x.line));
            self.edge(t, kind, x.line);
        }
        if x.name == "*" {
            return;
        }
        self.facts.reexports.push(Reexport {
            local_name: x.name.clone(),
            imported_name: x.local_name.unwrap_or(x.name),
            target,
            line: x.line,
        });
    }

    /// Records a load of `spec` that binds no import, when it names a
    /// third-party package (`external`), so the package is a dependency
    /// whatever form loads it.
    fn package_load(&mut self, external: bool, spec: &str, line: u32, kind: &'static str) {
        if external {
            self.facts.package_loads.push(PackageLoad {
                specifier: spec.to_owned(),
                line,
                kind,
            });
        }
    }

    /// The edge kind a re-export of `name` (`*` for all) links as.
    fn reexport_kind(&self, name: &str, export_kind: &str) -> &'static str {
        if name != "*" {
            return match export_kind {
                "type" => "type-named-reexport",
                UNKNOWN_LOAD => RECOVERED_NAMED_REEXPORT,
                _ => "named-reexport",
            };
        }
        if self.lang == Linker::Rust {
            return "rust-use";
        }
        match export_kind {
            "type" => "type-star-reexport",
            UNKNOWN_LOAD => RECOVERED_STAR_REEXPORT,
            _ => "star-reexport",
        }
    }

    /// `import { a } from './x'; export { a as b }`: an ES module binding is
    /// immutable, so exporting it re-exports `a` of `./x` under `b`.
    fn export_import_binding(&mut self, x: &octocode_engine::graph::GraphFactExport) {
        if self.lang != Linker::JsTs {
            return;
        }
        let local = x.local_name.as_deref().unwrap_or(&x.name);
        let forwarded = self
            .facts
            .imports
            .iter()
            .filter(|import| {
                import.local_name.as_deref() == Some(local)
                    && !matches!(import.imported_name.as_str(), "" | "*")
            })
            .map(|import| Reexport {
                local_name: x.name.clone(),
                imported_name: import.imported_name.clone(),
                target: import.target.clone(),
                line: x.line,
            })
            .collect::<Vec<_>>();
        self.facts.reexports.extend(forwarded);
    }

    pub(super) fn call(&mut self, mut c: octocode_engine::graph::GraphFactCall) {
        if self.lang == Linker::Rust {
            c.callee = rust_without_generic_arguments(&c.callee);
        }
        if c.kind == "dynamic-import" {
            let target = self.resolve(&c.callee, None, "*");
            let external = self.record(
                c.line,
                &c.callee,
                &target,
                false,
                super::super::edges::DYNAMIC_IMPORT,
            );
            self.package_load(
                external,
                &c.callee,
                c.line,
                super::super::edges::DYNAMIC_IMPORT,
            );
            if let Some(t) = target {
                self.edge(&t, super::super::edges::DYNAMIC_IMPORT, c.line);
                self.node.dynamic_only.insert(t.clone());
                self.b.namespace_targets.insert(t);
            }
            return;
        }
        let target = (self.lang == Linker::Rust)
            .then(|| {
                rust_call_target(
                    &c.callee,
                    &self.file,
                    &self.facts.imports,
                    self.inputs.known,
                    self.inputs.cargo_crates,
                    self.inputs.rust_modules,
                )
            })
            .flatten();
        // A qualified path (`crate::b::f()`) depends on its module as much
        // as a `use` does.
        if let Some(t) = target.as_ref().filter(|t| **t != self.file) {
            self.edge(t, super::super::edges::RUST_PATH, c.line);
            self.rust_path_use(t, &c.callee, c.line, true);
        }
        if self.lang == Linker::Rust && target.is_none() {
            self.note_unbound_path(&c.callee, c.line);
        }
        self.facts.calls.push(Call {
            id: c.id,
            binding: c.binding,
            source_range: c.source_range,
            parse_assessment: c.parse_assessment,
            caller_id: c.caller_id,
            callee: c.callee,
            line: c.line,
            kind: c.kind,
            receiver_type: c.receiver_type,
            term_binding: c.term_binding,
            bound_declaration: c.bound_declaration,
            self_template: c.self_template,
            macros: c.macros,
            receiver_call: c.receiver_call,
            receiver_declared: c.receiver_declared,
        });
    }

    /// A Rust qualified path outside call and `use` position depends on its
    /// module as a call does. One that names the current crate but does not
    /// resolve is an internal gap, never silently complete coverage, unless
    /// it names code above the snapshot root, as an import would.
    pub(super) fn path(&mut self, mut p: octocode_engine::graph::GraphFactPath) {
        p.path = rust_without_generic_arguments(&p.path);
        let target = match rust_inline_path(&p.path, p.inline_depth) {
            None => Some(self.file.clone()),
            Some(path) => rust_call_target(
                path,
                &self.file,
                &self.facts.imports,
                self.inputs.known,
                self.inputs.cargo_crates,
                self.inputs.rust_modules,
            ),
        };
        if target.is_none() && p.inline_depth == 0 {
            self.note_unbound_path(&p.path, p.line);
        }
        match &target {
            Some(t) if *t != self.file => {
                self.edge(t, super::super::edges::RUST_PATH, p.line);
                self.rust_path_use(t, &p.path, p.line, false);
            }
            Some(_) => {}
            None if rust_internal_specifier(&p.path) => {
                let outside = self.bare_target(&p.path) == BareTarget::OutsideRoot
                    || rust_super_escapes_root(&self.file, &p.path);
                if outside {
                    record_unresolved_internal(
                        self.b,
                        &self.file,
                        p.line,
                        &p.path,
                        true,
                        super::super::edges::RUST_PATH,
                        self.inputs.security,
                    );
                    return;
                }
                self.b.count_import(
                    ImportOutcome::UnresolvedInternal,
                    &self.file,
                    p.line,
                    &p.path,
                );
                self.b.diagnostics.push(Diagnostic {
                    import_kind: Some(super::super::edges::RUST_PATH),
                    ..Diagnostic::new(
                        self.file.clone(),
                        Some(p.line),
                        DiagnosticCode::UnresolvedInternal,
                        sanitize(
                            self.inputs.security,
                            &format!("Cannot link path {:?} (unresolvedInternal).", p.path),
                        ),
                    )
                });
            }
            None => {}
        }
    }

    /// A qualified Rust path or call names an item of the module `target`
    /// (`m::f()`, `m::T::new()`): the item it uses, so one a module
    /// re-exports is reached through its re-export. A path headed by a type
    /// (`T::new()`) names no module item: its `use` is the use.
    fn rust_path_use(&mut self, target: &str, path: &str, line: u32, reexport_only: bool) {
        let typed_head = path.starts_with(|c: char| c.is_uppercase());
        if let Some(item) = rust_path_item(path).filter(|_| !typed_head) {
            self.facts.rust_path_uses.push(RustPathUse {
                module: target.to_owned(),
                item: item.to_owned(),
                line,
                reexport_only,
            });
        }
    }

    /// Records the module path of a Rust call or value path (`m::f` names
    /// `f` in `m`) that resolved nowhere, when the file glob-imports an
    /// ancestor module and no `use` of its own binds the path's first
    /// segment: that glob may bind it ([`inherit_parent_globs`]).
    fn note_unbound_path(&mut self, path: &str, line: u32) {
        if let Some((prefix, _)) = path.rsplit_once("::") {
            let typed_head = path.starts_with(|c: char| c.is_uppercase());
            let item = rust_path_item(path)
                .filter(|_| !typed_head)
                .map(str::to_owned);
            self.note_unbound_head(prefix, item, line);
        }
    }

    /// A bare Rust type name written in type or struct-literal position
    /// (`fn f(_: &Saved)`, `Saved { .. }`) that nothing in the file binds or
    /// declares: a `use super::*` may bind it ([`inherit_parent_globs`]).
    pub(super) fn type_name(&mut self, t: octocode_engine::graph::GraphFactTypeName) {
        if !self.facts.declarations.iter().any(|d| d.name == t.name) {
            self.note_unbound_head(&t.name, None, t.line);
        }
    }

    /// Records `prefix`, a module path or a bare name, as unbound when the
    /// file glob-imports a module of its own crate (an ancestor's
    /// `use super::*` or a `use child::*`) and no `use` of its own binds its
    /// first segment.
    fn note_unbound_head(&mut self, prefix: &str, item: Option<String>, line: u32) {
        if !self
            .facts
            .imports
            .iter()
            .any(|import| import.imported_name == "*" && !import.inline && !import.external)
        {
            return;
        }
        let head = prefix.split("::").next().unwrap_or_default();
        let plain = !head.is_empty()
            && !matches!(head, "crate" | "self" | "super" | "Self")
            && head.chars().all(|c| c == '_' || c.is_alphanumeric());
        let bound = self.facts.imports.iter().any(|import| {
            import
                .local_name
                .as_deref()
                .unwrap_or(&import.imported_name)
                == head
        });
        if plain && !bound {
            self.facts
                .rust_unbound_paths
                .push((prefix.to_owned(), item, line));
        }
    }

    pub(super) fn common_js(&mut self, c: octocode_engine::graph::GraphFactCommonJs) {
        let Some(spec) = c.specifier else {
            let reason = c.reason.as_deref().unwrap_or("missing-native-provenance");
            self.b
                .count_import(ImportOutcome::Unsupported, &self.file, c.line, reason);
            self.b.diagnostics.push(Diagnostic::new(
                self.file.clone(),
                Some(c.line),
                DiagnosticCode::UnsupportedLinking,
                format!("CommonJS require cannot be linked ({reason})."),
            ));
            return;
        };
        let target = self.resolve(&spec, None, "*");
        let kind = if c.binding == "create-require" {
            "create-require"
        } else {
            "commonjs-require"
        };
        let external = self.record(c.line, &spec, &target, false, kind);
        self.package_load(external, &spec, c.line, kind);
        if let Some(t) = &target {
            self.edge(t, kind, c.line);
            self.b.namespace_targets.insert(t.clone());
        }
        for binding in c.bindings {
            self.facts.commonjs.push(CommonJsBinding {
                import: Import {
                    id: format!("commonjs:{}:{}:{}", spec, c.line, binding.local),
                    imported_name: binding
                        .imported
                        .unwrap_or_else(|| COMMONJS_WHOLE_EXPORT.to_owned()),
                    local_name: Some(binding.local),
                    specifier: spec.clone(),
                    line: c.line,
                    target: target.clone(),
                    external,
                    scope_id: None,
                    inline: false,
                    kind,
                },
                declaration_id: binding.declaration_id,
                scope: binding.scope,
            });
        }
    }
}
