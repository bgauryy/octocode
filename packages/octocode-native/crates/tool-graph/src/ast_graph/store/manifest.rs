//! The snapshot manifest written beside `graph.bin`.
use super::super::ANALYSIS_VERSION;
use super::*;
use octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION;

/// The snapshot was extracted, bound and projected by this binary's
/// representations: same fact schema, analysis and package version.
pub(super) fn analysis_current(manifest: &Value) -> bool {
    foreign_analysis(manifest).is_none()
        && manifest["factsSchemaVersion"] == GRAPH_FACTS_SCHEMA_VERSION
        && manifest["octocodeVersion"] == env!("CARGO_PKG_VERSION")
}

/// Why the snapshot of `manifest` is not evidence, when another analysis
/// version built it. Its facts were extracted and judged for redaction by
/// that version's rules (see [`ANALYSIS_VERSION`]), so a reader of this
/// version serves none of them: the snapshot is rebuilt, never read.
pub(super) fn foreign_analysis(manifest: &Value) -> Option<String> {
    let built = &manifest["analysisVersion"];
    if built == ANALYSIS_VERSION {
        return None;
    }
    let built = built.as_u64().map_or_else(
        || "an unrecorded analysis version".to_owned(),
        |version| format!("analysis version {version}"),
    );
    Some(format!(
        "snapshot {} was built by {built} and this Octocode reads only analysis version {ANALYSIS_VERSION}, so none of its facts are served. Rebuild it with graphBuild, then query the new snapshot",
        manifest["id"].as_str().unwrap_or_default()
    ))
}

/// The manifest's scan record with each fact under the name a request
/// gives it: the manifest stores a build's `parserGlobs` as
/// `languageGlobs`, the key every snapshot's scope is read back from.
pub(super) fn scan_named(scan: &Value) -> Value {
    let mut scan = scan.clone();
    if let Some(facts) = scan.as_object_mut()
        && let Some(globs) = facts.shift_remove("languageGlobs")
    {
        facts.insert("parserGlobs".to_owned(), globs);
    }
    scan
}

/// The manifest's scan record as an answer states it. The manifest keeps
/// every scan fact for freshness checks; an answer states `filesScanned`,
/// what the build read (its coverage, 0 when nothing in the scope is
/// graphable), and of the rest only those that carry information (a cut, an
/// exclude, a parser glob): zero counts, empty lists and false flags, older
/// snapshots' included, are left out.
pub(super) fn scan_disclosure(scan: &Value) -> Value {
    let mut scan = scan_named(scan);
    if let Some(facts) = scan.as_object_mut() {
        facts.retain(|key, value| {
            key == "filesScanned"
                || !(value == &json!(0)
                    || value == &json!(false)
                    || value.as_array().is_some_and(Vec::is_empty)
                    || value.as_object().is_some_and(serde_json::Map::is_empty))
        });
    }
    scan
}

/// The name a call spells: the last segment of a qualified callee or symbol
/// name (`x.f` and `A::f` spell `f`).
pub(super) fn spelled_name(name: &str) -> &str {
    name.rsplit(['.', ':']).next().unwrap_or(name)
}

/// Per declared symbol name, how many call sites spell it on a receiver no
/// edge resolves: the callers `graphQuery callers` cannot list. Only names a
/// symbol declares are kept, the only names a `callers` target can have.
fn unresolved_call_names(tables: &format::GraphTables) -> BTreeMap<String, u64> {
    let declared = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::Symbol)
        .map(|node| spelled_name(tables.str(node.name)))
        .collect::<BTreeSet<_>>();
    let mut counts = BTreeMap::<&str, u64>::new();
    for site in &tables.unresolved {
        if site.relation != format::EdgeKind::Calls {
            continue;
        }
        let name = spelled_name(tables.str(site.callee));
        if declared.contains(name) {
            *counts.entry(name).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(name, count)| (name.to_owned(), count))
        .collect()
}

/// Per calling node, how many of its call sites link no edge though their
/// name is declared in this repository (the in-repo reasons of
/// [`tables::INTERNAL_REASONS`]): the callees `graphQuery callees` cannot
/// list. `[node, count]` pairs in node order, so the manifest grows with the
/// callers that have such sites, not with their names.
fn unresolved_calls_from(tables: &format::GraphTables) -> Vec<[u64; 2]> {
    let mut counts = BTreeMap::<u32, u64>::new();
    for site in &tables.unresolved {
        if site.relation == format::EdgeKind::Calls
            && tables::INTERNAL_REASONS.contains(&tables.str(site.reason))
        {
            *counts.entry(site.src).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(node, count)| [u64::from(node), count])
        .collect()
}

/// The files holding an argument group the extractor could not read, each
/// with the declared names that code may use (`null`: any name), sorted by
/// file node: a symbol change reaches such a file only when its code names
/// the symbol.
fn unextracted_arguments_in(projection: &tables::Projection) -> Vec<Value> {
    projection
        .unextracted
        .iter()
        .map(|(file, names)| json!([file, names]))
        .collect()
}

/// [`unextracted_arguments_in`] of a manifest: the declared names a file's
/// unextracted argument groups may use.
#[derive(Default)]
pub(crate) struct Unextracted {
    /// `None`: any name.
    files: BTreeMap<u32, Option<BTreeSet<String>>>,
}

impl Unextracted {
    pub(super) fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Whether code of `file` that no edge covers may use a name of `names`.
    pub(super) fn may_name<'n>(&self, file: u32, mut names: impl Iterator<Item = &'n str>) -> bool {
        match self.files.get(&file) {
            None => false,
            Some(None) => true,
            Some(Some(used)) => names.any(|name| used.contains(name)),
        }
    }
}

pub(super) fn unextracted_arguments_of(manifest: &Value) -> Unextracted {
    let files = manifest["unextractedArgumentsIn"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let file = u32::try_from(entry[0].as_u64()?).ok()?;
            let names = entry[1].as_array().map(|names| {
                names
                    .iter()
                    .filter_map(|name| name.as_str().map(str::to_owned))
                    .collect()
            });
            Some((file, names))
        })
        .collect();
    Unextracted { files }
}

/// [`unresolved_calls_from`]'s count for `node` in a manifest.
pub(super) fn unresolved_calls_of(manifest: &Value, node: u32) -> u64 {
    let pairs = manifest["unresolvedCallsFrom"].as_array();
    let pairs = pairs.map_or(&[][..], Vec::as_slice);
    let node = u64::from(node);
    pairs
        .binary_search_by_key(&node, |pair| pair[0].as_u64().unwrap_or(u64::MAX))
        .ok()
        .and_then(|at| pairs[at][1].as_u64())
        .unwrap_or(0)
}

pub(super) struct ManifestInput<'a> {
    pub(super) id: &'a str,
    pub(super) scope: &'a str,
    pub(super) created_at: &'a str,
    pub(super) workspace: &'a Path,
    pub(super) built: &'a super::super::types::BuiltGraph,
    pub(super) files_scanned: usize,
    pub(super) projection: &'a tables::Projection,
    pub(super) argument_coverage: &'a Value,
    pub(super) bytes: usize,
    pub(super) digest: &'a str,
    pub(super) exclude: &'a [String],
    pub(super) walk: super::super::walk_scope::WalkScope,
    pub(super) language_globs: &'a BTreeMap<String, Vec<String>>,
    pub(super) inputs: &'a BTreeMap<String, String>,
    pub(super) input_proof: Option<&'a str>,
    pub(super) scope_proof: Option<&'a Value>,
    pub(super) build_ms: u64,
    pub(super) registry: octocode_engine::signatures::languages::RegistryView<'a>,
    pub(super) registry_identity: &'a str,
    /// The build's options, parsers and versions as one digest: equal over
    /// an unchanged tree means the same snapshot.
    pub(super) build_identity: &'a str,
    /// Root-relative directories the walk could not read, so the graph lacks
    /// everything below them.
    pub(super) unreadable_dirs: &'a [String],
    /// What the SCIP tier did, and why it failed when it did.
    pub(super) scip: (super::ScipStatus, Option<&'a str>),
}

pub(super) fn manifest(input: &ManifestInput) -> Value {
    let tables = &input.projection.tables;
    let mut by_node = BTreeMap::<&str, u64>::new();
    for node in &tables.nodes {
        *by_node.entry(node.kind.as_str()).or_default() += 1;
    }
    let mut by_edge = BTreeMap::<&str, u64>::new();
    for edge in &tables.edges {
        *by_edge.entry(edge.kind.as_str()).or_default() += 1;
    }
    let mut by_code = BTreeMap::<&str, u64>::new();
    for diag in &tables.diagnostics {
        *by_code.entry(tables.str(diag.code)).or_default() += 1;
    }
    let imports = input.projection.imports;
    let call_extraction = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.detail))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|language| {
            let mut capabilities = input
                .registry
                .select(language)
                .map(|entry| entry.capability().facts.calls);
            let capability = capabilities
                .next()
                .filter(|first| capabilities.all(|next| next == *first));
            (language, capability)
        })
        .collect::<BTreeMap<_, _>>();
    let mut manifest = json!({
        "kind": MANIFEST_KIND,
        "formatVersion": format::FORMAT_VERSION,
        "analysisVersion": ANALYSIS_VERSION,
        "factsSchemaVersion": GRAPH_FACTS_SCHEMA_VERSION,
        "registryIdentity": input.registry_identity,
        "buildIdentity": input.build_identity,
        "registryExtensions": input.registry.extensions(octocode_engine::text::analysis::AnalysisOperation::GraphFacts),
        "id": input.id,
        "scope": input.scope,
        "createdAt": input.created_at,
        "octocodeVersion": env!("CARGO_PKG_VERSION"),
        "root": input.built.root.to_string_lossy(),
        "workspace": input.workspace.to_string_lossy(),
        "graph": {"file": GRAPH_FILE, "bytes": input.bytes, "sha256": input.digest},
        "evidence": "syntax: imports are linked by module resolution; calls are name-resolved candidates tagged with resolution and confidence — confirm identity with lspSearch",
        "scan": {
            "filesScanned": input.files_scanned,
            "filesSkipped": input.built.files_skipped,
            "exclude": input.exclude,
            "parserFallback": input.built.parser_fallback.iter().map(|(file, producer)| {
                json!({"file": file, "producer": producer})
            }).collect::<Vec<_>>(),
            "markupNotLinked": input.built.markup_not_linked,
        },
        "counts": {
            "nodes": tables.nodes.len(),
            "edges": tables.edges.len(),
            "nodesByKind": by_node,
            "edgesByKind": by_edge,
        },
        "imports": {
            "resolved": imports[ImportOutcome::Resolved as usize],
            "external": imports[ImportOutcome::External as usize],
            "unresolvedInternal": imports[ImportOutcome::UnresolvedInternal as usize],
            "unsupported": imports[ImportOutcome::Unsupported as usize],
            "nonCode": imports[ImportOutcome::NonCode as usize],
            "unscanned": imports[ImportOutcome::Unscanned as usize],
        },
        "calls": input.projection.calls,
        "unresolvedCallNames": unresolved_call_names(tables),
        "unresolvedCallsFrom": unresolved_calls_from(tables),
        "unextractedArgumentsIn": unextracted_arguments_in(input.projection),
        "argumentCoverage": input.argument_coverage,
        "extraction": {
            "parsedFiles": input.built.parsed_files,
            "reusedFiles": input.built.reused_files,
            "resumedFiles": input.built.construction.resumed_files,
            "checkpointedFiles": input.built.construction.checkpointed_files,
            "spilledBytes": input.built.construction.spilled_bytes,
            "memoryOnlyFiles": input.built.construction.memory_only_files,
            "maxSpillBytes": input.built.construction.max_spill_bytes,
            "maxMemoryOnlyBytes": input.built.construction.max_memory_only_bytes,
        },
        "linkage": input.projection.calls.linkage(),
        "linkageByLanguage": input.projection.calls.linkage_by_language(),
        "callExtractionByLanguage": call_extraction,
        "languages": input.built.languages.iter().map(|(language, files, linking)| {
            json!({"language": language, "files": files, "linking": linking})
        }).collect::<Vec<_>>(),
        "diagnostics": {"total": tables.diagnostics.len(), "byCode": by_code},
        "inputs": input.inputs,
        "buildMs": input.build_ms,
    });
    if let Some(status) = input.scip.0.recorded() {
        manifest["scip"] = json!(status);
    }
    if let Some(failure) = input
        .scip
        .1
        .filter(|_| input.scip.0 == super::ScipStatus::Failed)
    {
        manifest["scipFailure"] = json!(failure);
    }
    if let Some(proof) = input.input_proof {
        manifest["inputProof"] = json!(proof);
    }
    if let Some(proof) = input.scope_proof {
        manifest["scopeProof"] = proof.clone();
    }
    if !tables.redacted.unresolved.is_empty() || !tables.redacted.witnesses.is_empty() {
        manifest["redactedRows"] = json!({
            "unresolved": tables.redacted.unresolved,
            "witnesses": tables.redacted.witnesses,
        });
    }
    if !input.unreadable_dirs.is_empty() {
        manifest["unreadableDirs"] = json!(input.unreadable_dirs);
    }
    // Where a read replaced text of a file, so a row of it can state its
    // range in the text a read shows (never served itself).
    if !input.built.hidden_spans.is_empty() {
        manifest["hiddenSpans"] = json!(input.built.hidden_spans);
    }
    if !input.language_globs.is_empty() {
        manifest["scan"]["languageGlobs"] = json!(input.language_globs);
    }
    if let Some(walk) = input.walk.disclosure() {
        manifest["scan"]["walk"] = walk;
    }
    manifest
}
