//! Per-file linking: Cargo and entry setup, the per-file pass, language
//! counts and fact diagnostics.
use super::*;

/// Whether Cargo linking was requested but has no manifest, and the
/// workspace crates `cargo metadata` names (empty when unavailable).
pub(super) fn cargo_linking(
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
pub(super) fn link_entry(
    built: &mut BuiltGraph,
    inputs: &LinkInputs,
    entry: octocode_engine::graph::GraphFactsTypedEntry,
    marks: super::super::store::classify::SourceMarks,
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
pub(super) fn is_rust(file: &str) -> bool {
    super::super::linking::Linker::of(file) == super::super::linking::Linker::Rust
}

pub(super) fn has_cargo_manifest(root: &Path, known: &BTreeSet<String>) -> bool {
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

pub(super) fn sanitize(security: &ContentSecurity, text: &str) -> String {
    security.sanitize_text(text, None).content
}

/// Per-scan inputs every file's linking reads.
pub(super) struct LinkInputs<'x> {
    pub(super) known: &'x BTreeSet<String>,
    pub(super) security: &'x ContentSecurity,
    pub(super) rust_cargo_unavailable: bool,
    pub(super) cargo_crates: &'x CargoCrates,
    pub(super) ctx: &'x ResolveContext,
    pub(super) packages: &'x PackageIndex,
}

pub(super) fn link_file(
    b: &mut BuiltGraph,
    inputs: &LinkInputs,
    file: String,
    p: RawFacts,
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
    pub(super) node: Node,
    pub(super) facts: FileFacts,
}
