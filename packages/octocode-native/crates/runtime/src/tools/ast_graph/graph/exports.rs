//! `FileLink`: exports, calls, path specifiers and CommonJS requires.
use super::*;

impl FileLink<'_, '_> {
    pub(super) fn export(&mut self, x: octocode_engine::graph::GraphFactExport) {
        let Some(spec) = x.source else {
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
                } else if x.export_kind == "type" {
                    "type-star-reexport"
                } else {
                    "star-reexport"
                };
                self.edge(&t, kind, x.line);
                self.b
                    .star_reexporters
                    .entry(t)
                    .or_default()
                    .push(self.file.clone());
            }
            return;
        }
        if let Some(t) = &target {
            let kind = if x.export_kind == "type" {
                "type-named-reexport"
            } else {
                "named-reexport"
            };
            self.edge(t, kind, x.line);
        }
        self.facts.reexports.push(Reexport {
            local_name: x.name.clone(),
            imported_name: x.local_name.unwrap_or(x.name),
            target,
        });
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
        let target = (self.lang == Linker::Rust && !self.inputs.rust_cargo_unavailable)
            .then(|| {
                rust_call_target(
                    &c.callee,
                    &self.file,
                    &self.facts.imports,
                    self.inputs.known,
                    self.inputs.cargo_crates,
                )
            })
            .flatten();
        // A qualified path (`crate::b::f()`) depends on its module as much
        // as a `use` does.
        if let Some(t) = target.as_ref().filter(|t| **t != self.file) {
            self.edge(t, super::super::edges::RUST_PATH, c.line);
        }
        self.facts.calls.push(Call {
            caller_id: c.caller_id,
            callee: c.callee,
            line: c.line,
            kind: c.kind,
            receiver_type: c.receiver_type,
        });
    }

    /// A Rust qualified path outside call and `use` position depends on its
    /// module as a call does. One that names the current crate but does not
    /// resolve is an internal gap, never silently complete coverage.
    pub(super) fn path(&mut self, p: octocode_engine::graph::GraphFactPath) {
        if self.inputs.rust_cargo_unavailable {
            // The import pass already reports the missing manifest.
            return;
        }
        let target = match rust_inline_path(&p.path, p.inline_depth) {
            None => Some(self.file.clone()),
            Some(path) => rust_call_target(
                path,
                &self.file,
                &self.facts.imports,
                self.inputs.known,
                self.inputs.cargo_crates,
            ),
        };
        match &target {
            Some(t) if *t != self.file => self.edge(t, super::super::edges::RUST_PATH, p.line),
            Some(_) => {}
            None if rust_internal_specifier(&p.path) => {
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
        self.record(c.line, &spec, &target, false);
        if let Some(t) = target {
            let kind = if c.binding == "create-require" {
                "create-require"
            } else {
                "commonjs-require"
            };
            self.edge(&t, kind, c.line);
            self.b.namespace_targets.insert(t);
        }
    }
}
