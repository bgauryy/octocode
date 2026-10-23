//! The snapshot manifest written beside `graph.bin`.
use super::*;

pub(super) struct ManifestInput<'a> {
    pub(super) id: &'a str,
    pub(super) scope: &'a str,
    pub(super) created_at: &'a str,
    pub(super) workspace: &'a Path,
    pub(super) built: &'a super::super::types::BuiltGraph,
    pub(super) projection: &'a tables::Projection,
    pub(super) bytes: usize,
    pub(super) digest: &'a str,
    pub(super) max_files: u32,
    pub(super) exclude_dir: &'a [String],
    pub(super) language_globs: &'a BTreeMap<String, Vec<String>>,
    pub(super) inputs: &'a BTreeMap<String, String>,
    pub(super) build_ms: u64,
}

/// The manifest's `scan.languageGlobs`: absent when no glob was given.
pub(super) fn language_globs_json(globs: &BTreeMap<String, Vec<String>>) -> Value {
    if globs.is_empty() {
        Value::Null
    } else {
        json!(globs)
    }
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
    let imports = input.built.imports;
    let mut manifest = json!({
        "kind": MANIFEST_KIND,
        "formatVersion": format::FORMAT_VERSION,
        "id": input.id,
        "scope": input.scope,
        "createdAt": input.created_at,
        "octocodeVersion": env!("CARGO_PKG_VERSION"),
        "root": input.built.root.to_string_lossy(),
        "workspace": input.workspace.to_string_lossy(),
        "graph": {"file": GRAPH_FILE, "bytes": input.bytes, "sha256": input.digest},
        "evidence": "syntax: imports are linked by module resolution; calls are name-resolved candidates tagged with resolution and confidence — confirm identity with lspSearch",
        "scan": {
            "filesScanned": input.built.facts.len(),
            "filesSkipped": input.built.files_skipped,
            // The file scan stopped at `maxFiles`; the edge cap is apart:
            // more files cannot lift it, and it leaves the scan complete.
            "truncated": input.built.truncated,
            "edgesCapped": input.built.edges_capped,
            "maxFiles": input.max_files,
            "excludeDir": input.exclude_dir,
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
        },
        "calls": input.projection.calls,
        "callInternalRecall": (input.projection.calls.internal_recall() * 1000.0).round() / 1000.0,
        "callRecallByLanguage": input.projection.calls.recall_by_language(),
        "languages": input.built.languages.iter().map(|(language, files, linking)| {
            json!({"language": language, "files": files, "linking": linking})
        }).collect::<Vec<_>>(),
        "diagnostics": {"total": tables.diagnostics.len(), "byCode": by_code},
        "inputs": input.inputs,
        "buildMs": input.build_ms,
    });
    if !input.language_globs.is_empty() {
        manifest["scan"]["languageGlobs"] = json!(input.language_globs);
    }
    manifest
}
