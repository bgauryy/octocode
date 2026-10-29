use super::*;
use octocode_engine::fact_scan::{FactsSource, GraphFactsMappedScan, GraphFactsView};
use octocode_engine::graph::GraphFactsTypedEntry;
use octocode_engine::types::FileSystemQueryResult;

pub(super) enum Kept {
    Spill(checkpoint::Kept),
    Memory(
        Box<(
            GraphFactsTypedEntry,
            super::super::classify::SourceMarks,
            capture::CapturedSource,
        )>,
    ),
}

impl Kept {
    pub(super) fn modules(&self) -> Option<(&str, &[octocode_engine::graph::GraphFactRustModule])> {
        match self {
            Self::Spill(kept) => kept
                .modules
                .as_ref()
                .map(|(file, modules)| (file.as_str(), modules.as_slice())),
            Self::Memory(kept) => (kept.0.facts.language == "rust").then_some((
                kept.0.relative_path.as_str(),
                kept.0.facts.modules.as_slice(),
            )),
        }
    }
    pub(super) fn restore(
        self,
        relative: &str,
        job: Option<&checkpoint::Job<'_>>,
    ) -> Result<
        (
            GraphFactsTypedEntry,
            super::super::classify::SourceMarks,
            capture::CapturedSource,
        ),
        ToolError,
    > {
        match self {
            Self::Spill(kept) => job
                .ok_or_else(|| {
                    ToolError::new(ErrorCode::ExecutionFailed, "missing graph checkpoint owner")
                })?
                .restore(relative, kept),
            Self::Memory(kept) => Ok(*kept),
        }
    }
}

// Each admitted source is bounded by MAX_GRAPH_SOURCE_BYTES. Four candidates
// cap concurrently retained source bytes at 4 MiB without changing engine policy.
const PARALLEL_FILES: usize = 4;

pub(super) fn scan(
    options: GraphFactsScanOptions,
    mut walk: FileSystemQueryResult,
    reuse: &octocode_engine::fact_scan::GraphFactsReuse<'_>,
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<Kept> + Sync),
    stop: &(dyn Fn() -> bool + Sync),
    registry: RegistryView<'_>,
) -> Result<GraphFactsMappedScan<Kept>, octocode_engine::fact_scan::GraphScanError> {
    let mut entries = std::mem::take(&mut walk.entries).into_iter();
    let mut out = GraphFactsMappedScan {
        kept: Vec::new(),
        parsed: 0,
        skipped: Vec::new(),
        files_skipped: 0,
        truncated: walk.was_capped,
        scan_resume: None,
        scan_frontier: walk.first_unlisted.clone(),
        pruned_dirs: walk.pruned_dirs.clone(),
    };
    loop {
        let batch = entries.by_ref().take(PARALLEL_FILES).collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }
        let walked = FileSystemQueryResult {
            entries: batch,
            ..Default::default()
        };
        let result = octocode_engine::fact_scan::scan_walked_graph_facts_with_registry(
            options.clone(),
            walked,
            FactsSource::CachedGraph(reuse),
            &|_: &str| true,
            map,
            stop,
            registry,
            octocode_engine::text::analysis::MapLimits::default(),
        )?;
        out.kept.extend(result.kept);
        out.parsed += result.parsed;
        out.files_skipped += result.files_skipped;
        out.skipped.extend(result.skipped);
    }
    out.kept
        .sort_unstable_by(|left, right| left.0.cmp(&right.0));
    out.skipped
        .sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(out)
}

pub(super) struct Extracted<'a> {
    pub(super) facts: GraphFactsMappedScan<Kept>,
    pub(super) job: Option<checkpoint::Job<'a>>,
    pub(super) reused_files: usize,
    /// What the kept files certainly project.
    pub(super) tally: super::super::limits::Tally,
}

pub(super) fn extract<'a>(
    scan: &GraphScan,
    root: &Path,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    cancel: &dyn CancellationCheck,
    registry: RegistryView<'_>,
) -> Result<Extracted<'a>, ToolError> {
    // The one default prune (a git-tracked `build/` is scanned, as every
    // local tool walks it); the credential stores are the path policy's.
    let options = GraphFactsScanOptions {
        path: root.to_string_lossy().into_owned(),
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
    let capture_error = std::sync::Mutex::new(None);
    let stop = || {
        cancel.check().is_err()
            || capture_error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some()
    };
    let scan_error = |error| ToolError::graph_scan(&error, cancel);
    let walk = octocode_engine::fact_scan::walk_graph_fact_files_with_registry(
        &options, &admit, &stop, registry,
    )
    .map_err(scan_error)?;
    let job = checkpoint::Job::open(scan, root, &walk.entries, paths, security, registry)?;
    let admission = super::scope::Admission::new(scan, root, &walk.entries);
    // Files past the per-file budget: the map declines them (a filtered
    // file to the scan), so they are named here as scan gaps.
    let declined = std::sync::Mutex::new(Vec::<String>::new());
    let reused_files = std::sync::atomic::AtomicUsize::new(0);
    let reuse = |context: &octocode_engine::fact_scan::GraphFactsReuseContext<'_>| {
        job.as_ref().and_then(|job| job.load(context))
    };
    let facts = self::scan(
        options,
        walk,
        &reuse,
        &|view: &octocode_engine::fact_scan::GraphFactsView<'_>| {
            if view.reused {
                reused_files.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let capture = || {
                // A file past the per-file budget is the scan's named gap
                // (`None`); the whole-snapshot budget still ends the scan.
                let source = match capture::capture(view, cancel, &scan.limits) {
                    Ok(source) => source,
                    Err(error) if error.code == ErrorCode::ScopeTooBroad => {
                        declined
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(view.relative_path.to_owned());
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                };
                // A file of another fact schema is never linked (`link_entry`).
                let linked = view.facts.schema_version
                    == octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION;
                admission.admit(&source, linked.then_some(view.facts.declarations.len()))?;
                match &job {
                    Some(job) => job.keep(view, &source).map(|kept| Some(Kept::Spill(kept))),
                    None => {
                        let marks = super::super::classify::SourceMarks::of(
                            view.relative_path,
                            view.content,
                        );
                        Ok(Some(Kept::Memory(Box::new((
                            view.to_entry(),
                            marks,
                            source,
                        )))))
                    }
                }
            };
            match capture() {
                Ok(kept) => kept,
                Err(error) => {
                    capture_error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get_or_insert(error);
                    None
                }
            }
        },
        &stop,
        registry,
    );
    if let Some(error) = capture_error
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
    {
        return Err(error);
    }
    let mut facts = facts.map_err(scan_error)?;
    let declined = declined
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    if !declined.is_empty() {
        facts
            .skipped
            .extend(declined.into_iter().map(|relative_path| {
                octocode_engine::types::GraphFactsScanDiagnostic {
                    relative_path,
                    code: octocode_engine::fact_scan::SOURCE_BUDGET_EXCEEDED.to_owned(),
                    message: "file's source correspondence exceeds the per-file budget".to_owned(),
                }
            }));
        facts
            .skipped
            .sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    }
    let reused_files = reused_files.load(std::sync::atomic::Ordering::Relaxed);
    Ok(Extracted {
        facts,
        job,
        reused_files,
        tally: admission.tally(),
    })
}
