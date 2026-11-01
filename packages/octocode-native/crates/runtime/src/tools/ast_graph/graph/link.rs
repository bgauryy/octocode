//! Per-file linking: Cargo and entry setup, the per-file pass, language
//! counts and fact diagnostics.
use super::*;
use octocode_engine::graph::{FileGraphNode, GraphFactsDocument};

/// The workspace crates `cargo metadata` names for every Cargo workspace
/// that encloses a scanned Rust file, with a diagnostic per workspace it
/// could not read and one for Rust files no manifest encloses (their imports
/// of other crates stay external; module paths still link by syntax).
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
            format!("Cargo metadata cargo-manifest-missing: {loose} Rust file(s) have no enclosing Cargo.toml; their imports of other crates stay external."),
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

/// Link one scanned file, unless its fact schema is unsupported.
pub(super) fn link_entry(
    built: &mut BuiltGraph,
    inputs: &LinkInputs,
    entry: octocode_engine::graph::GraphFactsTypedEntry,
    marks: super::super::classify::SourceMarks,
) {
    let file = normalize(&entry.relative_path);
    let parsed: GraphFactsDocument = entry.facts;
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
pub(super) fn is_rust(file: &str) -> bool {
    super::super::linking::Linker::of(file) == super::super::linking::Linker::Rust
}

pub(super) fn sanitize(security: &ContentSecurity, text: &str) -> String {
    security.sanitize_text(text, None).content
}

/// Per-scan inputs every file's linking reads.
pub(super) struct LinkInputs<'x> {
    pub rust_modules: &'x RustModules,
    pub(super) known: &'x BTreeSet<String>,
    pub(super) security: &'x ContentSecurity,
    pub(super) cargo_crates: &'x CargoCrates,
    pub(super) ctx: &'x ResolveContext,
    pub(super) packages: &'x PackageIndex,
}

pub(super) fn link_file(
    b: &mut BuiltGraph,
    inputs: &LinkInputs,
    file: String,
    p: GraphFactsDocument,
    counts: BTreeMap<String, u32>,
) {
    let lang = Linker::of(&file);
    let language = if p.language.is_empty() {
        octocode_engine::text::extension_of(&file, false, "")
    } else {
        p.language.clone()
    };
    count_language(b, &language, lang.label());
    push_fact_diagnostics(b, &file, &p.diagnostics, &language, lang, inputs.security);
    let facts = FileFacts {
        source_digest: p.source_digest.clone(),
        argument_sites: p.argument_sites,
        argument_scan_complete: !p.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                octocode_engine::graph::GraphFactCode::RecoveredParse
                    | octocode_engine::graph::GraphFactCode::ParseError
                    | octocode_engine::graph::GraphFactCode::ParseDeadline
                    | octocode_engine::graph::GraphFactCode::TraversalDeadline
            )
        }),
        reference_counts: counts,
        language,
        test_lines: p.test_lines.clone(),
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
