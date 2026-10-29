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
use octocode_contract_types::ErrorCode;
use octocode_engine::graph::{GraphFactCode, GraphFactDiagnostic};
use octocode_engine::signatures::languages::RegistryView;
use octocode_engine::text::{lexical, slash_path};
use octocode_engine::types::GraphFactsScanOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::PoisonError,
};

mod cache;
pub(crate) mod capture;
pub(super) mod checkpoint;
mod exports;
mod imports;
mod link;
mod paths;
mod resolve;
mod rust;
mod scan;
pub(crate) mod scope;
use link::*;
pub(crate) use paths::normalize;
pub(super) use paths::{climb, dirname, join, join_within_root, parent};
pub(super) use resolve::OUTSIDE_ROOT;
use resolve::*;
pub(super) use rust::rust_module_child_dir;
use rust::*;

/// What a graph build scans: the root, the pruned directories, the file
/// bound, parser overrides, and how Rust links.
#[derive(Clone, Debug, Default)]
pub(crate) struct GraphScan {
    pub checkpoint: Option<PathBuf>,
    pub force: bool,
    /// The scan root (validated under the path policy before the walk).
    pub root: PathBuf,
    /// Directory names pruned on top of the default excluded directories.
    pub exclude_dir: Vec<String>,
    pub max_files: u32,
    /// The snapshot bounds the scan and linking refuse past.
    pub limits: super::limits::Limits,
    /// AST parsers by root-relative glob.
    pub language_globs: BTreeMap<String, Vec<String>>,
    /// Link Rust through `cargo metadata` (else syntax only).
    pub cargo_metadata: bool,
    /// The configured `OCTOCODE_CARGO`; `None` runs `cargo` from PATH.
    pub cargo: Option<String>,
}

/// The linked graph of one scan.
#[cfg(test)]
pub(crate) fn build_graph(
    scan: &GraphScan,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<BuiltGraph, ToolError> {
    build_graph_with_registry(scan, paths, security, cancel, RegistryView::installed())
}

pub(crate) fn build_graph_with_registry(
    scan: &GraphScan,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    registry: RegistryView<'_>,
) -> Result<BuiltGraph, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let validated = paths.validate(&scan.root).map_err(ToolError::policy)?;
    let cancelled = ToolError::cancelled;
    let scan::Extracted {
        facts,
        job,
        reused_files,
        tally,
    } = scan::extract(
        scan,
        &validated.canonical,
        paths,
        security,
        cancel,
        registry,
    )?;
    cancel.check().map_err(cancelled)?;
    if let Some(job) = &job {
        job.linking()?;
    }
    // A discovered path is not linkable until its facts were actually parsed.
    // Otherwise an unread or oversized target becomes a false resolved edge.
    let known: BTreeSet<String> = facts
        .kept
        .iter()
        .map(|(relative, _)| normalize(relative))
        .collect();
    let mut built = BuiltGraph {
        construction: job
            .as_ref()
            .map(checkpoint::Job::progress)
            .unwrap_or_default(),
        reused_files,
        parsed_files: facts.parsed.saturating_sub(reused_files),
        root: validated.canonical,
        files_skipped: facts.files_skipped,
        truncated: facts.truncated,
        limits: scan.limits,
        tally,
        ..Default::default()
    };
    built.resolution_inputs = super::freshness::config_inputs(
        paths,
        &built.root,
        &super::freshness::scope_dirs(
            &built.root,
            known
                .iter()
                .map(String::as_str)
                .chain(facts.skipped.iter().map(|skip| skip.relative_path.as_str())),
        ),
    );
    let (rust_cargo_unavailable, cargo_crates) = cargo_linking(scan, &mut built, &known, cancel);
    let rust_modules = RustModules::build(
        facts.kept.iter().filter_map(|(_, kept)| kept.modules()),
        &known,
        &cargo_crates,
    );
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
        rust_modules: &rust_modules,
        ctx: &resolve_context,
        packages: &packages,
    };
    for (relative, kept) in facts.kept {
        cancel.check().map_err(cancelled)?;
        let (entry, marks, source) = kept.restore(&relative, job.as_ref())?;
        let file = normalize(&entry.relative_path);
        link_entry(&mut built, &inputs, entry, marks);
        if let Some(facts) = built.facts.get_mut(&file) {
            facts.source = source;
        }
        // Past a bound the snapshot cannot publish: refuse now, not after
        // projection, redaction and encoding.
        if let Err((table, bound)) = scan.limits.admit(&built.tally) {
            return Err(scope::too_broad(
                &built.root,
                known.iter().map(String::as_str),
                &format!(
                    "{} files are too many for one graph snapshot: linking the first {} projects more than the snapshot's {bound} {table} rows",
                    known.len(),
                    built.facts.len()
                ),
            ));
        }
    }
    diagnose_unlinked_path_imports(&mut built, security);
    built.diagnostics.sort();
    built.diagnostics.dedup();
    Ok(built)
}

pub(super) fn registry_identity(registry: RegistryView<'_>) -> Result<String, ToolError> {
    let entries = registry
        .entries()
        .map(|entry| (entry.cache_identity(), entry.capability()))
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&entries).map_err(|error| {
        ToolError::new(
            ErrorCode::ExecutionFailed,
            format!("Could not encode graph registry capabilities: {error}"),
        )
    })?;
    Ok(octocode_engine::digest::sha256(&bytes))
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

#[cfg(test)]
mod normalize_tests;

#[cfg(test)]
mod tests;
