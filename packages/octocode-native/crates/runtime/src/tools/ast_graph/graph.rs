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
use octocode_engine::text::{lexical, slash_path};
use octocode_engine::types::GraphFactsScanOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

mod exports;
mod imports;
mod link;
mod paths;
mod resolve;
mod rust;
use link::*;
pub(crate) use paths::normalize;
pub(super) use paths::{climb, dirname, join, join_within_root, parent};
use resolve::*;
pub(super) use rust::rust_module_child_dir;
use rust::*;

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
    let validated = paths.validate(&scan.root).map_err(ToolError::policy)?;
    let cancelled = ToolError::cancelled;
    // The one default prune (a git-tracked `build/` is scanned, as every
    // local tool walks it); the credential stores are the path policy's.
    let options = GraphFactsScanOptions {
        path: validated.canonical.to_string_lossy().into_owned(),
        exclude_dir: Some(scan.exclude_dir.clone()),
        default_exclude_dir: Some(crate::policy::prune::walk_names(true, &[])),
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

#[cfg(test)]
mod normalize_tests;

#[cfg(test)]
mod tests;
