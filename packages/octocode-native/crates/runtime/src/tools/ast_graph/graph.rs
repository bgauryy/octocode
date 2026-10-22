use super::aliases::ResolveContext;
use super::cargo::{CargoCrates, load_cargo_crates};
use super::js_paths::{ModulesByName, probe_js};
use super::linking::Linker;
use super::packages::{PackageIndex, PackageLink};
use super::types::*;
use crate::response::result::ToolError;
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_engine::graph::{GraphFactCode, GraphFactDiagnostic};
use octocode_engine::types::GraphFactsScanOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

/// What a graph build scans: the root, the pruned directories, the file
/// bound, parser overrides, and how Rust links.
#[derive(Clone, Debug, Default)]
pub(crate) struct GraphScan {
    /// The scan root (validated under the path policy before the walk).
    pub root: PathBuf,
    /// Directory names pruned on top of the default excluded directories.
    pub exclude_dir: Vec<String>,
    pub max_files: u32,
    /// AST parsers by root-relative glob.
    pub language_globs: BTreeMap<String, Vec<String>>,
    /// Link Rust through `cargo metadata` (else syntax only).
    pub cargo_metadata: bool,
    /// The configured `OCTOCODE_CARGO`; `None` runs `cargo` from PATH.
    pub cargo: Option<String>,
}

/// The linked graph of one scan.
pub(crate) fn build_graph(
    scan: &GraphScan,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<BuiltGraph, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let validated = paths
        .validate(&scan.root)
        .map_err(|error| ToolError::policy(error, "pathValidationFailed"))?;
    let cancelled = ToolError::cancelled;
    let mut exclude = crate::policy::prune::directories(true);
    exclude.extend(scan.exclude_dir.iter().cloned());
    let options = GraphFactsScanOptions {
        path: validated.canonical.to_string_lossy().into_owned(),
        exclude_dir: Some(exclude),
        default_exclude_dir: None,
        exclude: None,
        max_files: Some(scan.max_files.max(1)),
        skip_files: None,
        max_file_bytes: Some(octocode_engine::fact_scan::MAX_GRAPH_SOURCE_BYTES),
        language_globs: (!scan.language_globs.is_empty())
            .then(|| crate::tools::ast_rule::language_globs(&scan.language_globs)),
        ignore: octocode_engine::search::walk::IgnorePolicy::default(),
    };
    let discovery = paths.discovery_walk();
    let admit = |path: &Path, file_type| discovery.permits_entry(path, file_type);
    let stop = || cancel.check().is_err();
    let scan_error = |error| ToolError::graph_scan(&error, cancel);
    let walk = octocode_engine::fact_scan::walk_graph_fact_files(&options, &admit, &stop)
        .map_err(scan_error)?;
    let facts = octocode_engine::fact_scan::scan_walked_graph_facts(
        options,
        walk,
        octocode_engine::fact_scan::FactsSource::Graph,
        &|_: &str| true,
        &|view: &octocode_engine::fact_scan::GraphFactsView<'_>| {
            let marks = super::store::classify::SourceMarks::of(view.relative_path, view.content);
            Some((view.to_entry(), marks))
        },
        &stop,
    )
    .map_err(scan_error)?;
    cancel.check().map_err(cancelled)?;
    // A discovered path is not linkable until its facts were actually parsed.
    // Otherwise an unread or oversized target becomes a false resolved edge.
    let known: BTreeSet<String> = facts
        .kept
        .iter()
        .map(|(_, (entry, _))| normalize(&entry.relative_path))
        .collect();
    let mut built = BuiltGraph {
        root: validated.canonical,
        files_skipped: facts.files_skipped,
        truncated: facts.truncated,
        ..Default::default()
    };
    let (rust_cargo_unavailable, cargo_crates) = cargo_linking(scan, &mut built, &known, cancel);
    let resolve_context = ResolveContext::load(&built.root, &known, paths, security);
    let packages = PackageIndex::build(&built.root, &known, &|path| {
        super::aliases::read_config_text(paths, security, path)
    });
    if packages.go_module_missing() {
        built.diagnostics.push(Diagnostic::new(
            ".",
            None,
            DiagnosticCode::UnsupportedLinking,
            "No readable go.mod at or above the scan root, so Go module imports cannot be linked and dependents are unproven. Ingest the module root (the directory with go.mod).".into(),
        ));
    }
    for skipped in facts.skipped {
        built.diagnostics.push(Diagnostic::new(
            normalize(&skipped.relative_path),
            None,
            DiagnosticCode::ScanSkip,
            sanitize(security, &format!("{}: {}", skipped.code, skipped.message)),
        ));
    }
    let inputs = LinkInputs {
        known: &known,
        security,
        rust_cargo_unavailable,
        cargo_crates: &cargo_crates,
        ctx: &resolve_context,
        packages: &packages,
    };
    for (_, (entry, marks)) in facts.kept {
        cancel.check().map_err(cancelled)?;
        link_entry(&mut built, &inputs, entry, marks);
    }
    diagnose_unlinked_path_imports(&mut built, security);
    built.diagnostics.sort();
    built.diagnostics.dedup();
    Ok(built)
}

/// A bare JS/TS import no tsconfig `paths`/`baseUrl` or workspace package
/// links is no dependent edge, yet it can name a scanned file by path
/// (`vs/base/common/x` for `src/vs/base/common/x.ts`). Each one is recorded
/// against the file it names, so `dependents` of that file states it and
/// never silently lacks an importer a text search finds.
fn diagnose_unlinked_path_imports(b: &mut BuiltGraph, security: &ContentSecurity) {
    let js_ts = |file: &&String| Linker::of(file) == Linker::JsTs;
    let modules = ModulesByName::new(b.facts.keys().filter(js_ts).map(String::as_str));
    let mut found = Vec::new();
    for (importer, facts) in b.facts.iter().filter(|(file, _)| js_ts(file)) {
        for import in facts
            .imports
            .iter()
            .filter(|import| import.target.is_none())
        {
            for target in modules.named_by(&import.specifier) {
                if target == importer {
                    continue;
                }
                let message = format!(
                    "Import {:?} names {target} by a bare path no tsconfig paths/baseUrl or workspace package links, so it is not a dependent edge.",
                    import.specifier
                );
                found.push(Diagnostic {
                    target: Some(target.to_owned()),
                    ..Diagnostic::new(
                        importer.as_str(),
                        Some(import.line),
                        DiagnosticCode::UnlinkedPathImport,
                        sanitize(security, &message),
                    )
                });
            }
        }
    }
    b.diagnostics.extend(found);
}

/// Whether Cargo linking was requested but has no manifest, and the
/// workspace crates `cargo metadata` names (empty when unavailable).
fn cargo_linking(
    scan: &GraphScan,
    b: &mut BuiltGraph,
    known: &BTreeSet<String>,
    cancel: &dyn CancellationCheck,
) -> (bool, CargoCrates) {
    let rust_cargo_unavailable = scan.cargo_metadata
        && known.iter().any(|file| is_rust(file))
        && !has_cargo_manifest(&b.root, known);
    if rust_cargo_unavailable {
        b.diagnostics.push(Diagnostic::new(
            ".",
            None,
            DiagnosticCode::UnsupportedLinking,
            "Cargo metadata cargo-manifest-missing: No Cargo.toml was found at the scan root or known Rust-file ancestors. Ingest the crate root that contains Cargo.toml (not a nested src/ directory); crate:: imports cannot be resolved otherwise.".into(),
        ));
    }
    let cargo_crates = if scan.cargo_metadata && !rust_cargo_unavailable {
        match load_cargo_crates(&b.root, scan.cargo.as_deref(), cancel) {
            Ok(map) => map,
            Err(message) => {
                b.diagnostics.push(Diagnostic::new(
                    ".",
                    None,
                    DiagnosticCode::UnsupportedLinking,
                    format!("Cargo metadata failed: {message}"),
                ));
                CargoCrates::default()
            }
        }
    } else {
        CargoCrates::default()
    };
    (rust_cargo_unavailable, cargo_crates.within(&b.root))
}

/// Link one scanned file, unless its fact schema is unsupported.
fn link_entry(
    built: &mut BuiltGraph,
    inputs: &LinkInputs,
    entry: octocode_engine::graph::GraphFactsTypedEntry,
    marks: super::store::classify::SourceMarks,
) {
    let file = normalize(&entry.relative_path);
    let parsed: RawFacts = entry.facts;
    if parsed.schema_version != octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION {
        built.files_skipped += 1;
        built.diagnostics.push(Diagnostic::new(
            file,
            None,
            DiagnosticCode::FactsSchemaUnsupported,
            format!(
                "unsupported graph-fact schema version: {}",
                parsed.schema_version
            ),
        ));
        return;
    }
    let counts = entry
        .reference_counts
        .into_iter()
        .map(|x| (x.declaration_id, x.count))
        .collect();
    link_file(built, inputs, file.clone(), parsed, counts);
    if let Some(facts) = built.facts.get_mut(&file) {
        facts.digest = entry.content_digest;
        facts.stamp = entry.source_stamp;
        facts.marks = marks;
    }
}

/// Whether `file` is Rust source, by the grammar its extension selects.
fn is_rust(file: &str) -> bool {
    super::linking::Linker::of(file) == super::linking::Linker::Rust
}

fn has_cargo_manifest(root: &Path, known: &BTreeSet<String>) -> bool {
    if root.join("Cargo.toml").is_file() {
        return true;
    }
    known.iter().filter(|file| is_rust(file)).any(|file| {
        let mut directory = root.join(file).parent().map(Path::to_path_buf);
        while let Some(candidate) = directory {
            if !candidate.starts_with(root) {
                return false;
            }
            if candidate.join("Cargo.toml").is_file() {
                return true;
            }
            if candidate == root {
                return false;
            }
            directory = candidate.parent().map(Path::to_path_buf);
        }
        false
    })
}

fn sanitize(security: &ContentSecurity, text: &str) -> String {
    security.sanitize_text(text, None).content
}
fn extension(file: &str) -> &str {
    file.rsplit('.')
        .next()
        .filter(|x| !x.contains('/'))
        .unwrap_or("")
}
/// Per-scan inputs every file's linking reads.
struct LinkInputs<'x> {
    known: &'x BTreeSet<String>,
    security: &'x ContentSecurity,
    rust_cargo_unavailable: bool,
    cargo_crates: &'x CargoCrates,
    ctx: &'x ResolveContext,
    packages: &'x PackageIndex,
}

fn link_file(
    b: &mut BuiltGraph,
    inputs: &LinkInputs,
    file: String,
    p: RawFacts,
    counts: BTreeMap<String, u32>,
) {
    let lang = Linker::of(&file);
    let language = if p.language.is_empty() {
        extension(&file).to_owned()
    } else {
        p.language.clone()
    };
    count_language(b, &language, lang.label());
    push_fact_diagnostics(b, &file, &p.diagnostics, &language, lang, inputs.security);
    let facts = FileFacts {
        reference_counts: counts,
        language,
        ..Default::default()
    };
    let mut linker = FileLink {
        b,
        inputs,
        file,
        lang,
        node: Node::default(),
        facts,
    };
    linker.declarations(&p.edges, p.declarations);
    for import in p.imports {
        linker.import(import, &p.modules);
    }
    if linker.lang == Linker::Java {
        linker.java_same_package(&p.calls, &p.edges);
    }
    for export in p.exports {
        linker.export(export);
    }
    for call in p.calls {
        linker.call(call);
    }
    for require in p.common_js {
        linker.common_js(require);
    }
    for path in p.paths {
        linker.path(path);
    }
    let FileLink {
        b,
        file,
        node,
        facts,
        ..
    } = linker;
    b.facts.insert(file.clone(), facts);
    b.nodes.insert(file, node);
}

fn count_language(b: &mut BuiltGraph, language: &str, link: &str) {
    if let Some((_, files, _)) = b.languages.iter_mut().find(|(name, _, _)| name == language) {
        *files += 1;
    } else {
        b.languages.push((language.to_owned(), 1, link.to_owned()));
    }
}

fn push_fact_diagnostics(
    b: &mut BuiltGraph,
    file: &str,
    diagnostics: &[GraphFactDiagnostic],
    language: &str,
    lang: Linker,
    security: &ContentSecurity,
) {
    for diagnostic in diagnostics {
        if diagnostic.code == GraphFactCode::SyntaxOnly {
            // Every response already declares coverage.basis = syntactic.
            // Repeating this notice for each file hides actionable gaps.
            continue;
        }
        let code = if diagnostic.code.is_linking_only() {
            DiagnosticCode::UnsupportedLinking
        } else {
            DiagnosticCode::ParseRecovery
        };
        b.diagnostics.push(Diagnostic::new(
            file,
            diagnostic.line,
            code,
            sanitize(security, &diagnostic.message),
        ));
    }
    if lang == Linker::Unsupported {
        b.diagnostics.push(Diagnostic::new(
            file,
            None,
            DiagnosticCode::UnsupportedLinking,
            format!(
                "Import linking is unsupported for {language}; declarations are syntax facts only."
            ),
        ));
    }
}

/// One file's linking state: its graph node and facts while edges are added.
struct FileLink<'x, 'g> {
    b: &'g mut BuiltGraph,
    inputs: &'x LinkInputs<'x>,
    file: String,
    lang: Linker,
    node: Node,
    facts: FileFacts,
}

impl FileLink<'_, '_> {
    fn edge(&mut self, target: &str, kind: &str, line: u32) {
        add_edge(self.b, &mut self.node, target, kind, line);
    }

    /// Whether `line` lies inside a function or method body of this file
    /// (after its header line): code there runs on call.
    fn inside_function(&self, line: u32) -> bool {
        self.facts.declarations.iter().any(|d| {
            matches!(d.kind.as_str(), "function" | "method") && d.line < line && line <= d.end_line
        })
    }

    /// A bare JS/TS specifier naming project code (alias, workspace package,
    /// `#`/`@/`/`~/` convention): failing to link it is an internal gap.
    fn internal_bare(&self, spec: &str) -> bool {
        self.lang == Linker::JsTs
            && !spec.starts_with('.')
            && !spec.starts_with('/')
            && self.inputs.ctx.is_internal_bare_js(spec, &self.file)
    }

    fn resolve(&self, spec: &str, hint: Option<&str>, imported: &str) -> Option<String> {
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

    fn record(
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

    fn declarations(
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

    fn import(
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
            if kind == super::edges::PYTHON_IMPORT && self.inside_function(i.line) {
                kind = super::edges::PYTHON_LAZY_IMPORT;
            }
            self.edge(t, kind, i.line);
            if i.imported_name.as_deref() == Some("*") || self.lang == Linker::CFamily || splice {
                self.b.namespace_targets.insert(t.clone());
            }
        }
        self.facts.imports.push(Import {
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
    fn package_import(&mut self, i: octocode_engine::graph::GraphFactImport) {
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
    fn java_same_package(
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

    fn export(&mut self, x: octocode_engine::graph::GraphFactExport) {
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

    fn call(&mut self, c: octocode_engine::graph::GraphFactCall) {
        if c.kind == "dynamic-import" {
            let target = self.resolve(&c.callee, None, "*");
            self.record(c.line, &c.callee, &target, false);
            if let Some(t) = target {
                self.edge(&t, super::edges::DYNAMIC_IMPORT, c.line);
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
            self.edge(t, super::edges::RUST_PATH, c.line);
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
    fn path(&mut self, p: octocode_engine::graph::GraphFactPath) {
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
            Some(t) if *t != self.file => self.edge(t, super::edges::RUST_PATH, p.line),
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

    fn common_js(&mut self, c: octocode_engine::graph::GraphFactCommonJs) {
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

/// Upper bound on file-graph edges. File count is already capped by the scan
/// (`max_files` ≤ 50k), but per-file import counts are attacker-sized; without
/// an edge cap a pathological tree can exhaust memory in the edge maps and the
/// evidence graph. Hitting the cap degrades to a truncated (diagnosed) graph.
pub(super) const MAX_GRAPH_EDGES: u32 = 2_000_000;

#[cfg(test)]
thread_local! {
    /// The edge cap this test thread builds under (tests lower it).
    pub(super) static EDGE_CAP: std::cell::Cell<u32> = const { std::cell::Cell::new(MAX_GRAPH_EDGES) };
}

pub(super) fn edge_cap() -> u32 {
    #[cfg(test)]
    {
        EDGE_CAP.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        MAX_GRAPH_EDGES
    }
}

fn add_edge(b: &mut BuiltGraph, node: &mut Node, target: &str, kind: &str, line: u32) {
    // The cap counts distinct (target, kind) edges; a repeat of one only
    // lowers its evidence line to the file's first.
    let repeat = node
        .edges
        .get_mut(target)
        .and_then(|kinds| kinds.get_mut(kind))
        .map(|first| *first = (*first).min(line))
        .is_some();
    if !repeat {
        if b.edge_count >= edge_cap() {
            if !b.edges_capped {
                b.edges_capped = true;
                b.diagnostics.push(Diagnostic::new(
                    ".",
                    None,
                    DiagnosticCode::GraphEdgeCap,
                    format!(
                        "Edge collection stopped at the {}-edge cap; graph answers are partial. Narrow the scan root or exclude.",
                        edge_cap()
                    ),
                ));
            }
            return;
        }
        b.edge_count += 1;
        node.edges
            .entry(target.into())
            .or_default()
            .insert(kind.into(), line);
    }
}
/// A Rust `use` path that unambiguously targets the current crate, so failing to
/// resolve it means the intra-crate edge graph is incomplete — never a benign
/// external dependency. `crate::`/`self::`/`super::` and leading `::` qualify.
fn rust_internal_specifier(spec: &str) -> bool {
    if spec.starts_with("::") {
        return true;
    }
    let first = spec
        .trim_start_matches("::")
        .split("::")
        .next()
        .unwrap_or("");
    matches!(first, "crate" | "self" | "super")
}

/// A path specifier naming data, style, or asset content (`./package.json`,
/// `./app.css`, `./logo.svg?url`). Such imports are bundler/loader concerns,
/// never code-graph edges, so failing to link them is not a coverage gap.
/// Python is excluded: its dotted relative modules (`.utils.json`) are code.
fn is_non_code_specifier(spec: &str) -> bool {
    const NON_CODE: &[&str] = &[
        "json", "jsonc", "json5", "css", "scss", "sass", "less", "styl", "pcss", "svg", "png",
        "jpg", "jpeg", "gif", "webp", "avif", "ico", "bmp", "tif", "tiff", "woff", "woff2", "ttf",
        "otf", "eot", "mp3", "mp4", "webm", "wav", "ogg", "txt", "md", "html", "htm", "wasm",
        "node", "yaml", "yml", "toml", "graphql", "gql", "csv", "xml", "pdf",
    ];
    let path = spec.split(['?', '#']).next().unwrap_or(spec);
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && NON_CODE.contains(&ext.to_ascii_lowercase().as_str())
    })
}

/// Tallies one import resolution; returns whether an unlinked specifier names
/// an external package (as opposed to an unresolved or unsupported one).
/// `internal_bare` marks a bare specifier that names project code (alias or
/// workspace package), so an unlinked one is an internal gap.
fn record_resolution(
    b: &mut BuiltGraph,
    file: &str,
    line: u32,
    spec: &str,
    lang: Linker,
    target: &Option<String>,
    unsupported: bool,
    internal_bare: bool,
    security: &ContentSecurity,
) -> bool {
    if target.is_some() {
        b.count_import(ImportOutcome::Resolved, file, line, spec)
    } else if unsupported {
        b.count_import(ImportOutcome::Unsupported, file, line, spec);
        b.diagnostics.push(Diagnostic::new(
            file,
            Some(line),
            DiagnosticCode::UnsupportedLinking,
            sanitize(
                security,
                &format!("Cannot link import {spec:?} (unsupported)."),
            ),
        ))
    } else if lang != Linker::Python
        && (spec.starts_with('.') || spec.starts_with('/') || internal_bare)
        && is_non_code_specifier(spec)
    {
        b.count_import(ImportOutcome::NonCode, file, line, spec)
    } else if spec.starts_with('.')
        || spec.starts_with('/')
        || (lang == Linker::Rust && rust_internal_specifier(spec))
        || internal_bare
    {
        b.count_import(ImportOutcome::UnresolvedInternal, file, line, spec);
        b.diagnostics.push(Diagnostic::new(
            file,
            Some(line),
            DiagnosticCode::UnresolvedInternal,
            sanitize(
                security,
                &format!("Cannot link import {spec:?} (unresolvedInternal)."),
            ),
        ))
    } else {
        b.count_import(ImportOutcome::External, file, line, spec);
        return true;
    }
    false
}

fn resolve(
    spec: &str,
    importer: &str,
    lang: Linker,
    hint: Option<&str>,
    imported: &str,
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
    ctx: &ResolveContext,
) -> Option<String> {
    if lang == Linker::Python {
        return resolve_python(
            spec,
            importer,
            hint,
            imported,
            known,
            &ctx.python_roots_for(importer),
        )
        .or_else(|| {
            // An absolute import naming the scan root's own package.
            let local = ctx
                .python_package_local(spec)
                .filter(|_| hint == Some("python-absolute"))?;
            if local.is_empty() {
                return ["__init__.py", "__init__.pyi"]
                    .into_iter()
                    .flat_map(|init| {
                        [format!("{imported}.py"), format!("{imported}/{init}")]
                            .into_iter()
                            .chain([init.to_owned()])
                    })
                    .find(|file| known.contains(file));
            }
            resolve_python(local, importer, hint, imported, known, &["."])
        });
    }
    if lang == Linker::CFamily {
        let quoted = match hint {
            Some("c-relative") => true,
            Some("c-system") => false,
            _ => return None,
        };
        if spec.starts_with('/') {
            return None;
        }
        if quoted
            && let Some(p) = ctx.join_relative(dirname(importer), spec)
            && known.contains(&p)
        {
            return Some(p);
        }
        // Then the project's include search path (bounded list), then a
        // unique path-suffix match for build-system include roots.
        return ctx
            .include_dirs(quoted)
            .iter()
            .find_map(|dir| join_within_root(dir, spec).filter(|p| known.contains(p)))
            .or_else(|| ctx.header_by_suffix(spec, importer));
    }
    if lang == Linker::Rust {
        return resolve_rust(spec, importer, known, cargo_crates);
    }
    if lang != Linker::JsTs {
        return None;
    }
    if spec.starts_with('/') {
        return None;
    }
    if !spec.starts_with('.') {
        return ctx.resolve_bare_js(spec, importer, known);
    }
    probe_js(&ctx.join_relative(dirname(importer), spec)?, known)
}
/// The directory holding the importer's child modules: `foo/` for
/// `foo/mod.rs` (and crate roots), `foo/bar/` for `foo/bar.rs`.
fn rust_module_child_dir(importer: &str) -> String {
    let name = importer.rsplit_once('/').map_or(importer, |x| x.1);
    if matches!(name, "mod.rs" | "lib.rs" | "main.rs") {
        dirname(importer).to_owned()
    } else {
        importer.strip_suffix(".rs").unwrap_or(importer).to_owned()
    }
}

/// The importer's crate-root directory: the nearest ancestor holding a
/// `lib.rs`/`main.rs` in the scanned set; `None` when the scan holds none.
fn rust_crate_root(importer: &str, known: &BTreeSet<String>) -> Option<String> {
    let mut dir = dirname(importer);
    loop {
        if known.contains(&join(dir, "lib.rs")) || known.contains(&join(dir, "main.rs")) {
            return Some(dir.to_owned());
        }
        let parent = dirname(dir);
        if parent == dir || dir == "." {
            return None;
        }
        dir = parent;
    }
}

/// Without a scanned crate root: the importer's nearest ancestor named
/// `src` (a multi-crate workspace has many `src/` roots — anchoring to the
/// importer's own is what makes `crate::` resolvable outside a single-crate
/// scan), else the scan-relative `src`.
fn rust_src_dir(importer: &str) -> String {
    let mut dir = dirname(importer);
    loop {
        if dir == "src" || dir.ends_with("/src") {
            return dir.to_owned();
        }
        let parent = dirname(dir);
        if parent == dir || dir == "." {
            break;
        }
        dir = parent;
    }
    "src".into()
}

/// Map module-path segments under `base` to a file: trailing segments may be
/// items (types, functions) or globs rather than modules, so take the
/// longest prefix that names a real file; with no resolvable segment the
/// path denotes the base module itself (`use super::*;`).
/// Rust paths the linker can settle from the importing file alone:
/// - `#[path = "x.rs"] mod name;` resolves relative to the declaring file.
/// - `super::`/`self::` inside an inline module (`mod tests { use super::*; }`)
///   that does not climb past it refers to the same file.
fn rust_local_target(
    import: &octocode_engine::graph::GraphFactImport,
    file: &str,
    modules: &[octocode_engine::graph::GraphFactRustModule],
    known: &BTreeSet<String>,
) -> Option<String> {
    if import.import_kind == "module" {
        let name = import.imported_name.as_deref()?;
        let path = modules
            .iter()
            .find(|m| m.name == name && m.line == import.line && !m.inline)?
            .path
            .as_deref()?;
        let target = join_within_root(dirname(file), path)?;
        return known.contains(&target).then_some(target);
    }
    let depth = import.module_scope.as_ref().map_or(0, Vec::len);
    if depth == 0 {
        return None;
    }
    let segments = import.specifier.split("::").collect::<Vec<_>>();
    let climbs = match segments.first() {
        Some(&"self") => 0,
        Some(&"super") => segments.iter().take_while(|s| **s == "super").count(),
        _ => return None,
    };
    (climbs <= depth).then(|| file.to_owned())
}

/// A Rust path written inside `depth` inline modules: a `self::` path or
/// one whose `super`s stay within those modules names this file (`None`);
/// otherwise the path with the inline climbs removed, resolved from the
/// file's own module.
fn rust_inline_path(path: &str, depth: u32) -> Option<&str> {
    if depth == 0 {
        return Some(path);
    }
    if path.starts_with("self::") {
        return None;
    }
    let climbs = path.split("::").take_while(|s| *s == "super").count();
    if climbs == 0 {
        return Some(path);
    }
    if climbs <= depth as usize {
        return None;
    }
    let mut rest = path;
    for _ in 0..depth {
        rest = rest.strip_prefix("super::")?;
    }
    Some(rest)
}

/// A Rust `super::` path written inside inline modules climbs those first:
/// at inline depth `d`, the first `d` `super`s stay in the file, so only the
/// rest climb from the file's own module.
fn rust_scoped_specifier<'a>(
    import: &'a octocode_engine::graph::GraphFactImport,
    lang: Linker,
) -> std::borrow::Cow<'a, str> {
    let depth = import.module_scope.as_ref().map_or(0, Vec::len);
    if lang != Linker::Rust || depth == 0 {
        return std::borrow::Cow::Borrowed(&import.specifier);
    }
    let mut rest = import.specifier.as_str();
    for _ in 0..depth {
        match rest.strip_prefix("super::") {
            Some(tail) if tail == "super" || tail.starts_with("super::") => rest = tail,
            _ => return std::borrow::Cow::Borrowed(&import.specifier),
        }
    }
    std::borrow::Cow::Owned(rest.to_owned())
}

fn resolve_rust_module_prefix(
    base: &str,
    segments: &[&str],
    known: &BTreeSet<String>,
) -> Option<String> {
    let module_len = segments
        .iter()
        .take_while(|segment| !segment.is_empty() && !segment.contains(['{', '*']))
        .count();
    (1..=module_len)
        .rev()
        .find_map(|len| {
            let stem = join(base, &segments[..len].join("/"));
            [format!("{stem}.rs"), join(&stem, "mod.rs")]
                .into_iter()
                .find(|path| known.contains(path))
        })
        .or_else(|| {
            [
                format!("{base}.rs"),
                join(base, "mod.rs"),
                join(base, "lib.rs"),
                join(base, "main.rs"),
            ]
            .into_iter()
            .find(|path| known.contains(path))
        })
}

/// The file named by the module prefix of a Rust qualified callee. A first
/// segment bound by a `use`/`mod` in the calling file resolves through that
/// binding (`use crate::helper as h; h::run()` → `helper.rs`); any other
/// prefix resolves as a path from the calling file (`crate::`, `super::`,
/// a workspace crate, or a child module).
fn rust_call_target(
    callee: &str,
    file: &str,
    imports: &[Import],
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
) -> Option<String> {
    let (prefix, _) = callee.rsplit_once("::")?;
    let first = prefix.split("::").next()?;
    if prefix.is_empty() || first == "Self" {
        return None;
    }
    let bound = imports.iter().find(|import| {
        import.target.is_some()
            && import
                .local_name
                .as_deref()
                .unwrap_or(&import.imported_name)
                == first
    });
    match bound {
        Some(import) => match &prefix[first.len()..] {
            "" => import.target.clone(),
            rest => resolve_rust(
                &format!("{}{rest}", import.specifier),
                file,
                known,
                cargo_crates,
            ),
        },
        None => resolve_rust(prefix, file, known, cargo_crates),
    }
}

fn resolve_rust(
    spec: &str,
    importer: &str,
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
) -> Option<String> {
    let trimmed = spec.trim_end_matches(';');
    let segments = trimmed.split("::").collect::<Vec<_>>();
    let first = *segments.first().unwrap_or(&"");
    if !matches!(first, "crate" | "self" | "super" | "")
        && let Some(src) = cargo_crates.resolve(importer, first)
    {
        let rest = &segments[1..];
        let base = dirname(src);
        if rest.is_empty() {
            return known.contains(src).then(|| src.clone());
        }
        return resolve_rust_module_prefix(base, rest, known)
            .or_else(|| known.contains(src).then(|| src.clone()));
    }
    match first {
        "crate" => match rust_crate_root(importer, known) {
            Some(base) => resolve_rust_module_prefix(&base, &segments[1..], known),
            // The crate's root is above the scan: a path into the scan's
            // own modules names a file from the scan root.
            None => match cargo_crates.inside_scan(&segments[1..]) {
                Some(rest) => resolve_rust_module_prefix(".", rest, known),
                None => resolve_rust_module_prefix(&rust_src_dir(importer), &segments[1..], known),
            },
        },
        "self" => {
            let base = rust_module_child_dir(importer);
            resolve_rust_module_prefix(&base, &segments[1..], known)
        }
        "super" => {
            // Each `super` climbs one module level from the importer's own
            // module directory.
            let mut base = rust_module_child_dir(importer);
            let mut index = 0;
            while segments.get(index) == Some(&"super") {
                base = dirname(&base).to_owned();
                index += 1;
            }
            resolve_rust_module_prefix(&base, &segments[index..], known)
        }
        // Leading `::` names an external crate absolutely (2015-style);
        // nothing to link inside this scan unless cargo metadata knows it.
        "" => segments
            .get(1)
            .and_then(|name| cargo_crates.resolve(importer, name))
            .filter(|src| known.contains(*src))
            .cloned(),
        _ => {
            // Uniform-path fallback: a bare `use foo::…` can name a module the
            // importer itself declares (`mod foo;`), whose file lives in the
            // importer's child-module directory. Only look there — matching
            // the last path segment anywhere in the tree fabricates edges
            // between unrelated same-named modules.
            let child_dir = rust_module_child_dir(importer);
            (1..=segments.len()).rev().find_map(|len| {
                let stem = join(&child_dir, &segments[..len].join("/"));
                [format!("{stem}.rs"), join(&stem, "mod.rs")]
                    .into_iter()
                    .find(|path| known.contains(path))
            })
        }
    }
}
/// Python imports: relative ones climb from the importer's package;
/// absolute ones try each import root (`roots`, nearest first). With
/// `from pkg import name`, a submodule `pkg/name.py` wins over `pkg` itself.
fn resolve_python(
    spec: &str,
    importer: &str,
    hint: Option<&str>,
    imported: &str,
    known: &BTreeSet<String>,
    roots: &[&str],
) -> Option<String> {
    if !matches!(hint, Some("python-relative") | Some("python-absolute")) {
        return None;
    }
    let dots = spec.chars().take_while(|c| *c == '.').count();
    let module = spec[dots..].replace('.', "/");
    let bases = if dots > 0 {
        let mut base = dirname(importer);
        for _ in 1..dots {
            if base == "." {
                return None;
            }
            base = dirname(base);
        }
        vec![base]
    } else {
        roots.to_vec()
    };
    let submodule = (!imported.is_empty()
        && imported != "*"
        && imported.chars().all(|c| c == '_' || c.is_alphanumeric()))
    .then_some(imported);
    let module_files = |stem: &str| {
        [
            format!("{stem}.py"),
            format!("{stem}.pyi"),
            join(stem, "__init__.py"),
            join(stem, "__init__.pyi"),
        ]
        .into_iter()
        .find(|x| known.contains(x))
    };
    bases.into_iter().find_map(|base| {
        let stem = join_within_root(base, &module)?;
        submodule
            .and_then(|name| module_files(&join(&stem, name)))
            .or_else(|| module_files(&stem))
    })
}
pub(super) fn dirname(p: &str) -> &str {
    p.rsplit_once('/').map(|x| x.0).unwrap_or(".")
}
pub(super) fn join(a: &str, b: &str) -> String {
    normalize(&format!("{a}/{b}"))
}
pub(super) fn join_within_root(a: &str, b: &str) -> Option<String> {
    climb(Vec::new(), &format!("{a}/{b}")).map(|parts| parts.join("/"))
}
/// The relative `path` walked from `parts`; `None` when a `..` climbs above
/// them or `path` is absolute.
pub(super) fn climb(mut parts: Vec<String>, path: &str) -> Option<Vec<String>> {
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts)
}
pub(crate) fn normalize(p: &str) -> String {
    let replaced = p.replace('\\', "/");
    let mut parts = Vec::new();
    for c in Path::new(&replaced).components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(x) => parts.push(x.to_string_lossy().into_owned()),
            Component::RootDir => {}
            Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use crate::tools::ast_graph::test_support::{Fixture, known};
    use crate::tools::ast_graph::types::DiagnosticCode;

    fn cargo_fixture() -> Fixture {
        let root = Fixture::new();
        std::fs::write(
            root.0.as_path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\", \"b\", \"z_one\", \"z_two\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        for (name, dependency) in [
            ("a", "shared = { package = \"z_one\", path = \"../z_one\" }"),
            ("b", "shared = { package = \"z_two\", path = \"../z_two\" }"),
            ("z_one", ""),
            ("z_two", ""),
        ] {
            let directory = root.0.as_path().join(name);
            std::fs::create_dir_all(directory.join("src")).unwrap();
            std::fs::write(directory.join("src/lib.rs"), "pub fn item() {}\n").unwrap();
            std::fs::write(directory.join("Cargo.toml"), format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\n{dependency}\n"
            )).unwrap();
        }
        root
    }

    #[test]
    fn cargo_aliases_are_resolved_in_the_importing_package() {
        let fixture = cargo_fixture();
        let crates = super::load_cargo_crates(
            &fixture.0.as_path().canonicalize().unwrap(),
            None,
            &crate::execution::cancel::NeverCancel,
        )
        .unwrap();
        let known = [
            "a/src/lib.rs",
            "b/src/lib.rs",
            "z_one/src/lib.rs",
            "z_two/src/lib.rs",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for (importer, expected) in [
            ("a/src/lib.rs", "z_one/src/lib.rs"),
            ("b/src/lib.rs", "z_two/src/lib.rs"),
        ] {
            assert_eq!(
                super::resolve_rust("shared::item", importer, &known, &crates),
                Some(expected.to_owned())
            );
        }
        assert_eq!(
            super::resolve_rust("shared::item", "z_one/src/lib.rs", &known, &crates),
            None
        );
        assert_eq!(
            super::resolve_rust("z_two::item", "a/src/lib.rs", &known, &crates),
            None
        );
    }

    #[test]
    fn cargo_member_manifest_changes_refresh_dependency_aliases() {
        let fixture = cargo_fixture();
        let manifest = fixture.0.as_path().join("z_two/Cargo.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            format!("{text}shared = {{ package = \"z_one\", path = \"../z_one\" }}\n"),
        )
        .unwrap();
        // Warm up after any first-run lockfile creation, so only the member
        // manifest changes between the two observations below.
        super::load_cargo_crates(
            &fixture.0.as_path().canonicalize().unwrap(),
            None,
            &crate::execution::cancel::NeverCancel,
        )
        .unwrap();
        let before = super::load_cargo_crates(
            &fixture.0.as_path().canonicalize().unwrap(),
            None,
            &crate::execution::cancel::NeverCancel,
        )
        .unwrap();
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replace("shared =", "renamed =")).unwrap();
        let after = super::load_cargo_crates(
            &fixture.0.as_path().canonicalize().unwrap(),
            None,
            &crate::execution::cancel::NeverCancel,
        )
        .unwrap();
        let known = ["z_two/src/lib.rs", "z_one/src/lib.rs"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            super::resolve_rust("shared::item", "z_two/src/lib.rs", &known, &before),
            Some("z_one/src/lib.rs".to_owned())
        );
        assert_eq!(
            super::resolve_rust("renamed::item", "z_two/src/lib.rs", &known, &before),
            None
        );
        assert_eq!(
            super::resolve_rust("renamed::item", "z_two/src/lib.rs", &known, &after),
            Some("z_one/src/lib.rs".to_owned())
        );
    }

    #[test]
    fn python_parent_imports_climb_the_declared_number_of_packages() {
        let known = ["pkg/shared.py", "pkg/sub/shared.py", "shared.py"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        assert_eq!(
            super::resolve_python(
                "..shared",
                "pkg/sub/worker.py",
                Some("python-relative"),
                "run",
                &known,
                &["."],
            ),
            Some("pkg/shared.py".to_owned())
        );
        assert_eq!(
            super::resolve_python(
                "....shared",
                "pkg/sub/worker.py",
                Some("python-relative"),
                "run",
                &known,
                &["."],
            ),
            None,
            "relative imports cannot climb above the scan root"
        );
    }

    #[test]
    fn relative_javascript_import_cannot_reenter_after_escaping_scan_root() {
        let known = ["target.ts"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        assert_eq!(
            super::resolve(
                "../target",
                "entry.ts",
                super::Linker::JsTs,
                None,
                "*",
                &known,
                &Default::default(),
                &Default::default(),
            ),
            None,
        );
    }

    #[test]
    fn extensionless_javascript_import_links_supported_module_extensions() {
        let known = ["src/module.mts", "src/legacy.cts"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        for (specifier, expected) in [
            ("./module", "src/module.mts"),
            ("./legacy", "src/legacy.cts"),
        ] {
            assert_eq!(
                super::resolve(
                    specifier,
                    "src/entry.ts",
                    super::Linker::JsTs,
                    None,
                    "*",
                    &known,
                    &Default::default(),
                    &Default::default(),
                ),
                Some(expected.to_owned()),
            );
        }
    }

    #[test]
    fn unsupported_languages_do_not_use_javascript_package_resolution() {
        let known = ["src/index.ts"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        let packages =
            super::ResolveContext::default().with_package("example", ".", "src/index.ts");
        assert_eq!(
            super::resolve(
                "example",
                "main.go",
                super::Linker::Go,
                None,
                "*",
                &known,
                &Default::default(),
                &packages,
            ),
            None,
        );
    }

    #[test]
    fn rust_internal_paths_resolve_across_workspace_crates() {
        let known = [
            "crates/runtime/src/lib.rs",
            "crates/runtime/src/config/mod.rs",
            "crates/runtime/src/config/types.rs",
            "crates/runtime/src/tools/mod.rs",
            "crates/runtime/src/tools/local_fetch.rs",
            "crates/engine/src/lib.rs",
            "crates/engine/src/lsp.rs",
        ]
        .iter()
        .map(|path| (*path).to_owned())
        .collect::<std::collections::BTreeSet<_>>();
        let crates = super::CargoCrates::default();
        // `crate::` anchors to the importer's own crate root, not a global
        // `src/` — the multi-crate workspace case.
        assert_eq!(
            super::resolve_rust(
                "crate::config::types::ResolvedConfig;",
                "crates/runtime/src/tools/local_fetch.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/types.rs".to_owned())
        );
        // Trailing item segments fall back to the longest module prefix.
        assert_eq!(
            super::resolve_rust(
                "crate::config::PROTECTED_KEYS;",
                "crates/runtime/src/tools/local_fetch.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        // `super::` climbs one module level per segment; an item-only tail
        // resolves to the parent module file.
        assert_eq!(
            super::resolve_rust(
                "super::PROTECTED_KEYS;",
                "crates/runtime/src/config/types.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        assert_eq!(
            super::resolve_rust(
                "super::super::tools::local_fetch::Row;",
                "crates/runtime/src/config/types.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/tools/local_fetch.rs".to_owned())
        );
        // `use super::*;` denotes the parent module file itself.
        assert_eq!(
            super::resolve_rust(
                "super::*;",
                "crates/runtime/src/config/types.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        // Grouped imports resolve to the deepest real module prefix.
        assert_eq!(
            super::resolve_rust(
                "crate::config::{types, validation};",
                "crates/runtime/src/tools/local_fetch.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        // Importers in a different crate anchor to their own root.
        assert_eq!(
            super::resolve_rust(
                "crate::lsp::Pool;",
                "crates/engine/src/lsp.rs",
                &known,
                &crates
            ),
            Some("crates/engine/src/lsp.rs".to_owned())
        );
    }

    #[test]
    fn rust_uniform_path_fallback_only_links_the_importers_child_modules() {
        let known = ["src/a.rs", "src/thing.rs", "src/b.rs", "src/b/child.rs"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        let crates = super::CargoCrates::default();
        // A same-named file elsewhere in the tree must not become an edge for a
        // path that is not one of the importer's own child modules.
        assert_eq!(
            super::resolve_rust("unrelated::thing;", "src/a.rs", &known, &crates),
            None
        );
        // A module the importer declares (`mod child;`) lives in its child
        // directory and resolves.
        assert_eq!(
            super::resolve_rust("child::item;", "src/b.rs", &known, &crates),
            Some("src/b/child.rs".to_owned())
        );
        // Root-style importers (lib.rs/main.rs/mod.rs) keep sibling resolution.
        assert_eq!(
            super::resolve_rust("thing::item;", "src/lib.rs", &known, &crates),
            Some("src/thing.rs".to_owned())
        );
    }

    /// The graph of `root`, pruning `exclude_dir` too.
    fn fixture_graph(root: &std::path::Path, exclude_dir: &[&str]) -> super::BuiltGraph {
        let paths = crate::tools::test_support::workspace_policy(root);
        super::build_graph(
            &super::GraphScan {
                root: root.to_path_buf(),
                exclude_dir: exclude_dir.iter().map(|dir| (*dir).to_owned()).collect(),
                max_files: 50_000,
                ..Default::default()
            },
            &paths,
            &crate::security::ContentSecurity::new(),
            &crate::execution::cancel::NeverCancel,
        )
        .unwrap()
    }

    #[test]
    fn alias_looking_bare_specifiers_are_internal_gaps_not_external() {
        let mut b = super::BuiltGraph::default();
        let security = crate::security::ContentSecurity::new();
        let record = |b: &mut super::BuiltGraph, spec: &str, internal: bool| {
            super::record_resolution(
                b,
                "src/a.ts",
                1,
                spec,
                super::Linker::JsTs,
                &None,
                false,
                internal,
                &security,
            )
        };
        assert!(record(&mut b, "react", false), "third-party stays external");
        assert!(!record(&mut b, "@/missing", true));
        assert!(!record(&mut b, "@acme/core/package.json", true));
        assert_eq!(b.imports, [0, 1, 1, 0, 1]);
        assert!(b.diagnostics.iter().any(
            |d| d.code == DiagnosticCode::UnresolvedInternal && d.message.contains("@/missing")
        ));
    }

    #[test]
    fn tsconfig_aliases_and_workspace_packages_link_in_a_built_graph() {
        let dir = Fixture::new();
        let root = dir.0.as_path().canonicalize().unwrap();
        dir.write("tsconfig.json", "{ /* jsonc */ \"compilerOptions\": { \"paths\": { \"@app/*\": [\"./app/src/*\"], }, }, }");
        dir.write("lib/package.json", r#"{ "name": "@acme/lib", "main": "./dist/index.js", "exports": { ".": { "import": "./dist/index.js" } } }"#);
        dir.write("lib/src/index.ts", "export const lib = 1;\n");
        dir.write("app/src/util.ts", "export const util = 1;\n");
        dir.write("app/src/main.ts", "import { util } from '@app/util';\nimport { lib } from '@acme/lib';\nimport { gone } from '@app/gone';\nimport React from 'react';\nexport const all = [util, lib, gone, React];\n");
        let built = fixture_graph(&root, &[]);
        let imports = &built.facts["app/src/main.ts"].imports;
        let target = |spec: &str| {
            imports
                .iter()
                .find(|import| import.specifier == spec)
                .map(|import| (import.target.clone(), import.external))
        };
        assert_eq!(
            target("@app/util"),
            Some((Some("app/src/util.ts".to_owned()), false))
        );
        assert_eq!(
            target("@acme/lib"),
            Some((Some("lib/src/index.ts".to_owned()), false))
        );
        assert_eq!(target("@app/gone"), Some((None, false)));
        assert_eq!(target("react"), Some((None, true)));
        assert_eq!(built.imports[..3], [2, 1, 1]);
    }

    #[test]
    fn python_absolute_imports_use_src_layout_roots_and_submodules() {
        let known = known(&[
            "proj/src/pkg/__init__.py",
            "proj/src/pkg/sub.py",
            "proj/src/pkg/nested/__init__.py",
            "proj/app.py",
        ]);
        let roots = ["proj/src", "proj", "."];
        let resolve = |spec: &str, imported: &str| {
            super::resolve_python(
                spec,
                "proj/app.py",
                Some(if spec.starts_with('.') {
                    "python-relative"
                } else {
                    "python-absolute"
                }),
                imported,
                &known,
                &roots,
            )
        };
        assert_eq!(
            resolve("pkg", "*").as_deref(),
            Some("proj/src/pkg/__init__.py")
        );
        assert_eq!(
            resolve("pkg", "sub").as_deref(),
            Some("proj/src/pkg/sub.py")
        );
        assert_eq!(
            resolve("pkg", "nested").as_deref(),
            Some("proj/src/pkg/nested/__init__.py")
        );
        // A non-module name falls back to the package itself.
        assert_eq!(
            resolve("pkg", "function_name").as_deref(),
            Some("proj/src/pkg/__init__.py")
        );
        assert_eq!(resolve("missing", "*"), None);
    }

    #[test]
    fn c_includes_search_project_include_dirs() {
        let dir = Fixture::new();
        let root = dir.0.as_path().canonicalize().unwrap();
        dir.write("include/proj/api.h", "int api(void);\n");
        dir.write("third/include/dep.h", "int dep(void);\n");
        dir.write("compile_commands.json", serde_json::json!([{"directory": root, "command": "cc -Ithird/include -c src/main.c", "file": "src/main.c"}]).to_string());
        dir.write("src/main.c", "#include \"proj/api.h\"\n#include <dep.h>\n#include <stdio.h>\nint main(void) { return api() + dep(); }\n");
        let built = fixture_graph(&root, &[]);
        let imports = &built.facts["src/main.c"].imports;
        let target = |spec: &str| {
            imports
                .iter()
                .find(|import| import.specifier == spec)
                .and_then(|import| import.target.clone())
        };
        assert_eq!(target("proj/api.h").as_deref(), Some("include/proj/api.h"));
        assert_eq!(target("dep.h").as_deref(), Some("third/include/dep.h"));
        assert_eq!(target("stdio.h"), None);
    }

    /// A graph scan walks under the one ignore policy (it has no
    /// `includeIgnored`): `.gitignore`, nested negations and
    /// `.git/info/exclude` leave files out of every scan.
    #[test]
    fn ignore_rules_always_apply() {
        let dir = Fixture::new();
        let root = dir.0.as_path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".git/info")).unwrap();
        dir.write(".git/info/exclude", "local-only.ts\n");
        dir.write(".gitignore", "generated/\n*.gen.ts\n");
        dir.write("pkg/.gitignore", "scratch/\n!keep.gen.ts\n");
        for file in [
            "src/main.ts",
            "generated/api.ts",
            "src/model.gen.ts",
            "pkg/keep.gen.ts",
            "pkg/scratch/tmp.ts",
            "local-only.ts",
        ] {
            dir.write(file, "export const x = 1;\n");
        }
        let filtered = fixture_graph(&root, &["pkg"]);
        assert_eq!(filtered.facts.keys().collect::<Vec<_>>(), ["src/main.ts"],);
        let nested = fixture_graph(&root, &[]);
        assert_eq!(
            nested.facts.keys().collect::<Vec<_>>(),
            ["pkg/keep.gen.ts", "src/main.ts"],
            "nested negation re-includes; nested dir rule prunes"
        );
    }

    #[test]
    fn link_file_keeps_heritage_and_call_kinds() {
        use octocode_engine::graph::{GraphFactCall, GraphFactEdge, GraphFactsDocument};
        let document = GraphFactsDocument {
            schema_version: octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION,
            language: "typescript".into(),
            edges: vec![
                GraphFactEdge {
                    from: "decl:A".into(),
                    to: "ns.Base".into(),
                    relation: "extends".into(),
                    line: 3,
                    ..Default::default()
                },
                GraphFactEdge {
                    from: "decl:A".into(),
                    to: "Shape".into(),
                    relation: "implements".into(),
                    line: 3,
                    ..Default::default()
                },
                GraphFactEdge {
                    from: "decl:A".into(),
                    to: "x".into(),
                    relation: "contains".into(),
                    line: 4,
                    ..Default::default()
                },
            ],
            calls: vec![
                GraphFactCall {
                    callee: "Widget".into(),
                    kind: "renders".into(),
                    line: 5,
                    ..Default::default()
                },
                GraphFactCall {
                    callee: "Injectable".into(),
                    kind: "decorates".into(),
                    caller_id: Some("decl:A".into()),
                    line: 2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut built = super::BuiltGraph::default();
        let known = known(&["a.ts"]);
        let security = crate::security::ContentSecurity::new();
        let inputs = super::LinkInputs {
            known: &known,
            security: &security,
            rust_cargo_unavailable: false,
            cargo_crates: &super::CargoCrates::default(),
            ctx: &super::ResolveContext::default(),
            packages: &Default::default(),
        };
        super::link_file(
            &mut built,
            &inputs,
            "a.ts".into(),
            document,
            Default::default(),
        );
        let facts = &built.facts["a.ts"];
        assert_eq!(
            facts
                .heritage
                .iter()
                .map(|h| (
                    h.decl_id.as_str(),
                    h.relation.as_str(),
                    h.target.as_str(),
                    h.line
                ))
                .collect::<Vec<_>>(),
            [
                ("decl:A", "extends", "ns.Base", 3),
                ("decl:A", "implements", "Shape", 3)
            ]
        );
        assert_eq!(
            facts
                .calls
                .iter()
                .map(|c| (c.callee.as_str(), c.kind.as_str(), c.caller_id.is_some()))
                .collect::<Vec<_>>(),
            [
                ("Widget", "renders", false),
                ("Injectable", "decorates", true)
            ]
        );
    }

    /// Facts of another schema version (v1 carried text diagnostics) are
    /// never linked: the file is skipped and says why.
    #[test]
    fn facts_of_an_older_schema_are_skipped_not_linked() {
        let mut built = super::BuiltGraph::default();
        let known = known(&["a.ts"]);
        let security = crate::security::ContentSecurity::new();
        let inputs = super::LinkInputs {
            known: &known,
            security: &security,
            rust_cargo_unavailable: false,
            cargo_crates: &super::CargoCrates::default(),
            ctx: &super::ResolveContext::default(),
            packages: &Default::default(),
        };
        let entry = octocode_engine::graph::GraphFactsTypedEntry {
            relative_path: "a.ts".into(),
            content_digest: "digest".into(),
            source_stamp: None,
            facts: octocode_engine::graph::GraphFactsDocument {
                schema_version: octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION - 1,
                ..Default::default()
            },
            reference_counts: Vec::new(),
        };
        super::link_entry(&mut built, &inputs, entry, Default::default());
        assert!(built.nodes.is_empty());
        assert_eq!(built.files_skipped, 1);
        assert_eq!(
            built.diagnostics[0].code,
            DiagnosticCode::FactsSchemaUnsupported
        );
    }

    /// A Rust module-attribute note keeps its line in the diagnostic's
    /// `line`, and the message carries no line text.
    #[test]
    fn a_module_attribute_note_names_its_line() {
        let dir = crate::tools::test_support::Fixture::new();
        dir.write("lib.rs", "fn a() {}\n#[custom]\nmod t;\n");
        dir.write("t.rs", "fn b() {}\n");
        let built = fixture_graph(&dir.0, &[]);
        let note = built
            .diagnostics
            .iter()
            .find(|d| d.file == "lib.rs" && d.code == DiagnosticCode::UnsupportedLinking)
            .unwrap_or_else(|| panic!("{:?}", built.diagnostics));
        assert_eq!(note.line, Some(3), "the `mod` line: {note:?}");
        assert_eq!(
            note.message,
            "unsupported Rust conditional or custom module attributes"
        );
    }

    #[test]
    fn cargo_metadata_stdout_is_drained_while_the_child_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let crates = super::load_cargo_crates(root, None, &crate::execution::cancel::NeverCancel)
            .expect("cargo metadata");
        assert_eq!(
            crates
                .resolve("src/lib.rs", "octocode_native")
                .map(String::as_str),
            Some("src/lib.rs")
        );
    }
}
