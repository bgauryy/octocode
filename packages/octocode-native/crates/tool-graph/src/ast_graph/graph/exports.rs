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
        self.record(x.line, &spec, &target, self.lang == Linker::Unsupported);
        if let Some(t) = &target {
            self.facts.reexport_lines.push((t.clone(), x.line));
        }
        if x.name == "*" {
            if let Some(t) = target {
                let kind = if self.lang == Linker::Rust {
                    "rust-use"
                } else {
                    match x.export_kind.as_str() {
                        "type" => "type-star-reexport",
                        UNKNOWN_LOAD => RECOVERED_STAR_REEXPORT,
                        _ => "star-reexport",
                    }
                };
                self.edge(&t, kind, x.line);
            }
            return;
        }
        if let Some(t) = &target {
            let kind = match x.export_kind.as_str() {
                "type" => "type-named-reexport",
                UNKNOWN_LOAD => RECOVERED_NAMED_REEXPORT,
                _ => "named-reexport",
            };
            self.edge(t, kind, x.line);
        }
        self.facts.reexports.push(Reexport {
            local_name: x.name.clone(),
            imported_name: x.local_name.unwrap_or(x.name),
            target,
            line: x.line,
        });
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

    pub(super) fn call(&mut self, c: octocode_engine::graph::GraphFactCall) {
        if c.kind == "dynamic-import" {
            let target = self.resolve(&c.callee, None, "*");
            self.record(c.line, &c.callee, &target, false);
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
    pub(super) fn path(&mut self, p: octocode_engine::graph::GraphFactPath) {
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
                if let Some(item) = rust_path_item(&p.path) {
                    self.facts
                        .rust_path_uses
                        .push((t.clone(), item.to_owned(), p.line));
                }
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
                self.b.diagnostics.push(Diagnostic::new(
                    self.file.clone(),
                    Some(p.line),
                    DiagnosticCode::UnresolvedInternal,
                    sanitize(
                        self.inputs.security,
                        &format!("Cannot link path {:?} (unresolvedInternal).", p.path),
                    ),
                ));
            }
            None => {}
        }
    }

    /// Records the module path of a Rust call or value path (`m::f` names
    /// `f` in `m`) that resolved nowhere, when the file glob-imports an
    /// ancestor module and no `use` of its own binds the path's first
    /// segment: that glob may bind it ([`inherit_parent_globs`]).
    fn note_unbound_path(&mut self, path: &str, line: u32) {
        let Some((prefix, _)) = path.rsplit_once("::") else {
            return;
        };
        if !self
            .facts
            .imports
            .iter()
            .any(|import| import.imported_name == "*" && ancestor_glob(&import.specifier))
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
                .push((prefix.to_owned(), line));
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
        let external = self.record(c.line, &spec, &target, false);
        if let Some(t) = &target {
            let kind = if c.binding == "create-require" {
                "create-require"
            } else {
                "commonjs-require"
            };
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
                },
                declaration_id: binding.declaration_id,
                scope: binding.scope,
            });
        }
    }
}
