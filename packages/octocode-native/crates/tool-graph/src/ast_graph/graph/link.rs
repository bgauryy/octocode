//! Per-file linking: Cargo and entry setup, the per-file pass, language
//! counts and fact diagnostics.
use super::*;
use crate::ast_graph::linking::LinkerGraph;
use octocode_engine::graph::{FileGraphNode, GraphFactsDocument};

/// The workspace crates `cargo metadata` names for every Cargo workspace
/// that encloses a scanned Rust file, with a diagnostic per workspace it
/// could not read and one for Rust files no manifest encloses (what their
/// imports of other crates name stays unknown; module paths still link by syntax).
pub(super) fn cargo_linking(
    scan: &GraphScan,
    b: &mut BuiltGraph,
    known: &BTreeSet<String>,
    cancel: &dyn CancellationCheck,
) -> CargoCrates {
    let rust = known
        .iter()
        .map(String::as_str)
        .filter(|file| is_rust(file));
    let (manifests, loose) = enclosing_manifests(&b.root, rust);
    if loose > 0 {
        b.diagnostics.push(Diagnostic::new(
            ".",
            None,
            DiagnosticCode::UnsupportedLinking,
            format!("Cargo metadata cargo-manifest-missing: {loose} Rust file(s) have no enclosing Cargo.toml; what their imports of other crates name stays unknown."),
        ));
    }
    let (crates, failures) = load_cargo_crates(&b.root, &manifests, scan.cargo.as_deref(), cancel);
    for failure in failures {
        b.diagnostics.push(Diagnostic::new(
            ".",
            None,
            DiagnosticCode::UnsupportedLinking,
            format!("Cargo metadata failed: {failure}"),
        ));
    }
    crates.within(&b.root)
}

/// Link one scanned file. Its facts are of the current schema: extraction
/// writes it, and every reused or checkpointed entry passes
/// `GraphFactsReuseContext::accepts`, which checks it.
pub(super) fn link_entry(
    built: &mut BuiltGraph,
    inputs: &LinkInputs,
    entry: octocode_engine::graph::GraphFactsTypedEntry,
    marks: super::super::classify::SourceMarks,
) {
    let file = normalize(&entry.relative_path);
    let parsed: GraphFactsDocument = entry.facts;
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
pub(super) fn is_rust(file: &str) -> bool {
    super::super::linking::Linker::of(file) == super::super::linking::Linker::Rust
}

pub(super) fn sanitize(security: &ContentSecurity, text: &str) -> String {
    security.sanitize_text(text, None).content.into_owned()
}

/// Per-scan inputs every file's linking reads.
pub(super) struct LinkInputs<'x> {
    pub rust_modules: &'x RustModules,
    pub(super) known: &'x BTreeSet<String>,
    pub(super) security: &'x ContentSecurity,
    pub(super) cargo_crates: &'x CargoCrates,
    pub(super) ctx: &'x ResolveContext,
    pub(super) packages: &'x PackageIndex,
    /// The scanned JS/TS modules, for a bare specifier a file's path ends in.
    pub(super) js_modules: &'x super::super::js_paths::ModulesByName<'x>,
}

pub(super) fn link_file(
    b: &mut BuiltGraph,
    inputs: &LinkInputs,
    file: String,
    p: GraphFactsDocument,
    counts: BTreeMap<String, u32>,
) {
    let language = if p.language.is_empty() {
        octocode_engine::text::extension_of(&file, false, "")
    } else {
        p.language.clone()
    };
    // The grammar the file was read with, a `parserGlobs` rule's for an
    // extension no grammar claims, decides its linking and its label.
    let lang = Linker::of_read(&file, &language);
    let label = super::super::languages::canonical_language(&file, &language);
    count_language(b, &label, lang.label());
    if p.diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == GraphFactCode::ParserFallback)
    {
        let producer = p
            .source_templates
            .units
            .first()
            .map_or_else(String::new, |unit| unit.adapter.id.clone());
        b.parser_fallback.push((file.clone(), producer));
    }
    push_fact_diagnostics(b, &file, &p.diagnostics, &label, lang, inputs.security);
    let (argument_sites, argument_counts) = linkable_arguments(p.argument_sites);
    let facts = FileFacts {
        source_digest: p.source_digest.clone(),
        argument_sites,
        argument_counts,
        argument_scan_complete: !p.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                octocode_engine::graph::GraphFactCode::RecoveredParse
                    | octocode_engine::graph::GraphFactCode::ParseError
                    | octocode_engine::graph::GraphFactCode::ParserFallback
                    | octocode_engine::graph::GraphFactCode::ParseDeadline
                    | octocode_engine::graph::GraphFactCode::TraversalDeadline
            )
        }),
        reference_counts: counts,
        language,
        test_lines: p.test_lines.clone(),
        go_package: p.go_package.clone(),
        declared_types: p.declared_types,
        conditional_exports: p.conditional_exports,
        ..Default::default()
    };
    let mut linker = FileLink {
        b,
        inputs,
        file,
        lang,
        node: FileGraphNode::default(),
        facts,
    };
    linker.declarations(&p.edges, p.declarations);
    let imports = if lang == Linker::Rust {
        rust_binding_order(p.imports)
    } else {
        p.imports
    };
    for import in imports {
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
        mut facts,
        ..
    } = linker;
    facts.shrink_to_fit();
    b.facts.insert(file.clone(), facts);
    b.nodes.insert(file, node);
}

/// The sites projection links and the counts of the whole scan. A value that
/// is not a reference links nothing, nor does a site left with no slot; most
/// sites are that, and each holds its own occurrence ids until projection.
fn linkable_arguments(
    mut sites: Vec<octocode_engine::graph::GraphFactArgumentSite>,
) -> (
    Vec<octocode_engine::graph::GraphFactArgumentSite>,
    ArgumentCounts,
) {
    use octocode_engine::graph::{GraphArgumentGroupObservation, GraphArgumentObservation};
    let mut counts = ArgumentCounts::default();
    for site in &mut sites {
        counts.sites += 1;
        counts.groups += site.groups.len() as u64;
        site.groups
            .retain_mut(|group| match &mut group.observation {
                GraphArgumentGroupObservation::Unsupported { .. } => {
                    counts.unsupported_groups += 1;
                    true
                }
                GraphArgumentGroupObservation::Slots { slots } => {
                    counts.slots += slots.len() as u64;
                    slots.retain(|slot| match slot.observation {
                        GraphArgumentObservation::Reference { .. } => {
                            counts.direct_references += 1;
                            true
                        }
                        GraphArgumentObservation::NonReference { .. } => {
                            counts.non_reference_values += 1;
                            false
                        }
                        GraphArgumentObservation::Unsupported { .. } => {
                            counts.unsupported_slots += 1;
                            true
                        }
                    });
                    slots.shrink_to_fit();
                    !slots.is_empty()
                }
            });
        site.groups.shrink_to_fit();
    }
    sites.retain(|site| !site.groups.is_empty());
    sites.shrink_to_fit();
    (sites, counts)
}

pub(super) fn count_language(b: &mut BuiltGraph, language: &str, link: &str) {
    if let Some((_, files, _)) = b.languages.iter_mut().find(|(name, _, _)| name == language) {
        *files += 1;
    } else {
        b.languages.push((language.to_owned(), 1, link.to_owned()));
    }
}

pub(super) fn push_fact_diagnostics(
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
        } else if diagnostic.code == GraphFactCode::ParserFallback {
            DiagnosticCode::ParserFallback
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
pub(super) struct FileLink<'x, 'g> {
    pub(super) b: &'g mut BuiltGraph,
    pub(super) inputs: &'x LinkInputs<'x>,
    pub(super) file: String,
    pub(super) lang: Linker,
    pub(super) node: FileGraphNode,
    pub(super) facts: FileFacts,
}

/// Rust `use` declarations in an order that links each binding before a
/// `use` whose path starts at it (`use scanned::streamed;` before `use
/// streamed::walk_cut;`, wherever each is written), else in source order.
fn rust_binding_order(
    imports: Vec<octocode_engine::graph::GraphFactImport>,
) -> Vec<octocode_engine::graph::GraphFactImport> {
    let bound = |import: &octocode_engine::graph::GraphFactImport| {
        import
            .local_name
            .clone()
            .or_else(|| import.imported_name.clone())
    };
    let mut pending = imports;
    let mut ordered = Vec::with_capacity(pending.len());
    loop {
        // A `use` waits while another pending one binds its first segment.
        let waits = (0..pending.len())
            .map(|at| {
                let head = pending[at].specifier.split("::").next().unwrap_or_default();
                !matches!(head, "" | "crate" | "self" | "super")
                    && pending.iter().enumerate().any(|(other, import)| {
                        other != at && bound(import).as_deref() == Some(head)
                    })
            })
            .collect::<Vec<_>>();
        if waits.iter().all(|wait| *wait) {
            // Nothing left, or bindings that wait on each other.
            ordered.extend(pending);
            return ordered;
        }
        let mut kept = Vec::new();
        for (import, wait) in pending.into_iter().zip(waits) {
            if wait {
                kept.push(import);
            } else {
                ordered.push(import);
            }
        }
        pending = kept;
    }
}
