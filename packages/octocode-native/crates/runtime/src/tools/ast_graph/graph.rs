use super::aliases::{BareTarget, ResolveContext};
use super::cargo::{CargoCrates, enclosing_manifests, load_cargo_crates};
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
use octocode_engine::types::GraphFactsScanOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::PoisonError,
};

mod cache;
pub(crate) mod capture;
pub(super) mod checkpoint;
mod exports;
mod imports;
mod link;
mod markup;
mod resolve;
mod rust;
mod scan;
pub(crate) mod scope;
mod turn;
use super::paths::{dirname, join, join_within_root, normalize};
#[cfg(test)]
pub(crate) mod peaks;
#[cfg(test)]
pub(super) use checkpoint::inject_secret_scan_fault;
use link::*;
use resolve::*;
pub(super) use rust::rust_module_child_dir;
use rust::*;

/// What a graph build scans: the root, the caller's exclusions, the file
/// bound, parser overrides, and how Rust links.
#[derive(Clone, Debug, Default)]
pub(crate) struct GraphScan {
    pub checkpoint: Option<PathBuf>,
    /// Rebuild the extraction checkpoint instead of resuming or reusing it.
    pub refresh: bool,
    /// The scan root (validated under the path policy before the walk).
    pub root: PathBuf,
    /// The caller's path globs to skip, relative to the root.
    pub exclude: Vec<String>,
    /// The default prune, ignore files and dot entries the walk keeps or lifts.
    pub walk: super::walk_scope::WalkScope,
    /// The snapshot bounds the scan and linking refuse past.
    pub limits: super::limits::Limits,
    /// AST parsers by root-relative glob.
    pub language_globs: BTreeMap<String, Vec<String>>,
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
        withheld,
        job,
        reused_files,
        markup,
        tally,
    } = {
        let turn = turn::ExtractionTurn::new(cancel, scan.checkpoint.is_some());
        match scan::extract(scan, &validated.canonical, paths, security, &turn, registry) {
            Ok(extracted) => extracted,
            Err(_) if turn.ended() => {
                return Ok(paused(scan, validated.canonical, turn.walked()));
            }
            Err(error) => return Err(error),
        }
    };
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
        limits: scan.limits,
        tally,
        ..Default::default()
    };
    if let Some(notice) = withheld.notice() {
        built.diagnostics.push(Diagnostic::new(
            ".",
            None,
            DiagnosticCode::PathPolicyWithheld,
            format!("{notice} Contents were not scanned; absence and unused-code conclusions are unproven for this scan."),
        ));
    }
    for directory in &facts.pruned_dirs {
        built.diagnostics.push(Diagnostic::new(
            normalize(directory),
            None,
            DiagnosticCode::DefaultExcluded,
            "Default-excluded directory: contents were not scanned; absence and unused-code conclusions are unproven under it.".into(),
        ));
    }
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
    let cargo_crates = cargo_linking(scan, &mut built, &known, cancel);
    let rust_modules = RustModules::build(
        facts.kept.iter().filter_map(|(_, kept)| kept.modules()),
        &known,
        &cargo_crates,
    );
    let resolve_context = ResolveContext::load(&built.root, &known, paths, security);
    let packages = PackageIndex::build(&built.root, &known, &|path| {
        super::aliases::read_config_text(paths, security, path)
    })
    .with_go_files(facts.kept.iter().filter_map(|(_, kept)| kept.go_file()));
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
    let js_modules = ModulesByName::new(
        known
            .iter()
            .filter(|file| Linker::of(file) == Linker::JsTs)
            .map(String::as_str),
    );
    let inputs = LinkInputs {
        known: &known,
        security,
        cargo_crates: &cargo_crates,
        rust_modules: &rust_modules,
        ctx: &resolve_context,
        packages: &packages,
        js_modules: &js_modules,
    };
    // Restoring a spilled file decodes its artifact: a window of files is
    // restored in parallel, then linked one by one in scan order, so the
    // graph is the same at any width. The window bounds the decoded facts
    // held at once.
    let width = crate::tools::workers::workers(facts.kept.len(), None);
    let mut kept = facts.kept.into_iter();
    loop {
        cancel.check().map_err(cancelled)?;
        let mut window = kept.by_ref().take(width * 4).map(Some).collect::<Vec<_>>();
        if window.is_empty() {
            break;
        }
        let restored = crate::tools::workers::each_parallel(&mut window, width, |slot| {
            slot.take()
                .map(|(relative, kept)| kept.restore(&relative, job.as_ref()))
        });
        for restored in restored.into_iter().flatten() {
            cancel.check().map_err(cancelled)?;
            link_restored(&mut built, &inputs, &known, scan, restored?)?;
        }
    }
    markup::link_markup(
        &mut built,
        &inputs,
        markup,
        scan.limits.build_bytes as u64,
        cancel,
    )?;
    diagnose_unlinked_path_imports(&mut built, &js_modules, security);
    built.diagnostics.sort();
    built.diagnostics.dedup();
    Ok(built)
}

/// Path markers whose files panic while linked (tests only).
#[cfg(test)]
static LINK_FAULTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Makes linking panic on every file whose path contains `marker`.
#[cfg(test)]
pub(super) fn inject_link_fault(marker: &str) {
    LINK_FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(marker.to_owned());
}

#[cfg(test)]
fn link_fault(file: &str) {
    let faults = LINK_FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert!(
        !faults.iter().any(|marker| file.contains(marker.as_str())),
        "injected link panic"
    );
}

/// The key under which a link failure's `calls` names its torn file, for
/// the ingest's exclude retry (it never reaches a response).
pub(crate) const TORN_FILE: &str = "tornFile";

/// Links one restored file into `built`, refusing once the snapshot passes
/// a bound: refused now, not after projection, redaction and encoding.
fn link_restored(
    built: &mut BuiltGraph,
    inputs: &LinkInputs<'_>,
    known: &BTreeSet<String>,
    scan: &GraphScan,
    (entry, marks, source): (
        octocode_engine::graph::GraphFactsTypedEntry,
        super::classify::SourceMarks,
        capture::CapturedSource,
    ),
) -> Result<(), ToolError> {
    let file = normalize(&entry.relative_path);
    // A panic mid-link leaves the graph torn, so the build cannot publish:
    // it fails, naming the file and where linking panicked.
    octocode_engine::contain::catch(octocode_engine::contain::Phase::Link, || {
        #[cfg(test)]
        link_fault(&file);
        link_entry(built, inputs, entry, marks);
    })
    .map_err(|panic| {
        let mut error = ToolError::new(
            ErrorCode::ExecutionFailed,
            format!("graph build failed on {file}: {panic}; no snapshot was published"),
        );
        // The ingest turns this into its retry without the file.
        error.calls = Some(Box::new(serde_json::json!({ TORN_FILE: file })));
        error
    })?;
    if let Some(facts) = built.facts.get_mut(&file) {
        facts.source = source;
    }
    if let Err(over) = scan.limits.admit(&built.tally) {
        return Err(scope::too_broad(
            &built.root,
            &super::build_scope::BuildScope::of_scan(scan),
            known.iter().map(String::as_str),
            &format!(
                "{} files are too many for one graph snapshot: linking the first {}, they {over}",
                known.len(),
                built.facts.len()
            ),
        ));
    }
    Ok(())
}

/// A checkpointed build whose extraction used its share of the call's
/// budget: what it committed resumes on the next call of the same input.
fn paused(scan: &GraphScan, root: PathBuf, files: usize) -> BuiltGraph {
    let committed_files = scan
        .checkpoint
        .as_deref()
        .and_then(|base| std::fs::read_dir(checkpoint::job_path(base, &root)).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".fact"))
        .count();
    BuiltGraph {
        paused: Some(ExtractionPause {
            committed_files,
            remaining_files: files.saturating_sub(committed_files),
        }),
        root,
        ..Default::default()
    }
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
fn diagnose_unlinked_path_imports(
    b: &mut BuiltGraph,
    modules: &ModulesByName<'_>,
    security: &ContentSecurity,
) {
    let js_ts = |file: &&String| Linker::of(file) == Linker::JsTs;
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
