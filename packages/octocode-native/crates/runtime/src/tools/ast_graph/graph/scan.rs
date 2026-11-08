use super::*;
use octocode_engine::fact_scan::{FactsSource, GraphFactsMappedScan, GraphFactsView};
use octocode_engine::graph::GraphFactsTypedEntry;
use octocode_engine::types::FileSystemQueryResult;

pub(super) enum Kept {
    Spill(Box<checkpoint::Kept>),
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
    /// A Go file's name, package clause and build exclusion.
    pub(super) fn go_file(&self) -> Option<(&str, Option<&str>, bool)> {
        match self {
            Self::Spill(kept) => kept
                .go_file
                .as_ref()
                .map(|(file, package, excluded)| (file.as_str(), package.as_deref(), *excluded)),
            Self::Memory(kept) => (kept.0.facts.language == "go").then_some((
                kept.0.relative_path.as_str(),
                kept.0.facts.go_package.as_deref(),
                kept.0.facts.build_excluded,
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
                .restore(relative, *kept),
            Self::Memory(kept) => Ok(*kept),
        }
    }
}

/// The graph facts of every walked file, in one engine scan: its files are
/// parsed in parallel on the process's shared parse pool, so the files in
/// flight (and the source bytes they hold, each within the build budget
/// [`super::super::limits::Limits::build_bytes`]) are bounded by that pool's threads, and
/// concurrent builds share its cores. The walk's own bounds (`was_capped`,
/// the unlisted frontier, pruned directories) are the scan's.
pub(super) fn scan(
    options: GraphFactsScanOptions,
    mut walk: FileSystemQueryResult,
    source: FactsSource<'_>,
    map: &(dyn Fn(&GraphFactsView<'_>) -> Option<Kept> + Sync),
    stop: &(dyn Fn() -> bool + Sync),
    registry: RegistryView<'_>,
    source_map: octocode_engine::text::analysis::MapLimits,
) -> Result<GraphFactsMappedScan<Kept>, octocode_engine::fact_scan::GraphScanError> {
    let walked = FileSystemQueryResult {
        entries: std::mem::take(&mut walk.entries),
        ..Default::default()
    };
    let result = octocode_engine::fact_scan::scan_walked_graph_facts_with_registry(
        options,
        walked,
        source,
        &|_: &str| true,
        map,
        stop,
        registry,
        source_map,
    )?;
    let mut out = GraphFactsMappedScan {
        kept: result.kept,
        parsed: result.parsed,
        passed_over: result.passed_over,
        skipped: result.skipped,
        files_skipped: result.files_skipped,
        truncated: walk.was_capped,
        scan_resume: None,
        scan_frontier: walk.first_unlisted.clone(),
        pruned_dirs: walk.pruned_dirs.clone(),
    };
    out.kept
        .sort_unstable_by(|left, right| left.0.cmp(&right.0));
    out.skipped
        .sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(out)
}

pub(super) struct Extracted<'a> {
    pub(super) facts: GraphFactsMappedScan<Kept>,
    pub(super) withheld: crate::policy::discovery::Withheld,
    pub(super) job: Option<checkpoint::Job<'a>>,
    pub(super) reused_files: usize,
    /// The markup files the scan's walk leaves out: no graph node holds them.
    pub(super) markup: Vec<octocode_engine::types::FileSystemEntry>,
    /// What the kept files certainly project.
    pub(super) tally: super::super::limits::Tally,
}

/// The scan options of a graph build of `root`: the one default prune (a
/// git-tracked `build/` is scanned, as every local tool walks it) unless
/// `defaultExcludes:false`, the ignore policy `includeIgnored` and
/// `includeHidden` name; the credential stores are the path policy's.
fn scan_options(scan: &GraphScan, root: &Path) -> GraphFactsScanOptions {
    GraphFactsScanOptions {
        path: root.to_string_lossy().into_owned(),
        default_exclude_dir: Some(crate::policy::prune::walk_names(
            scan.walk.default_excludes,
            &[],
        )),
        exclude: (!scan.exclude.is_empty()).then(|| scan.exclude.clone()),
        // No file cap (the snapshot bounds refuse a build that cannot
        // publish); a set `max_files` keeps the scan off the time budget.
        max_files: Some(u32::MAX),
        skip_files: None,
        // A source larger than everything one build may hold can never be
        // kept: past it the file is a named skip, not a refused build. Any
        // smaller file is parsed; its parse work bound, the per-file source
        // budget and the build's admission decide what it costs.
        max_file_bytes: Some(scan.limits.build_bytes as u64),
        language_globs: (!scan.language_globs.is_empty())
            .then(|| crate::tools::ast_rule::language_globs(&scan.language_globs)),
        ignore: scan.walk.ignore,
    }
}

/// Files the map leaves out, past the per-file budget or failing their
/// secret scan (a filtered file to the scan), named as scan gaps.
#[derive(Default)]
struct Gaps(std::sync::Mutex<Vec<octocode_engine::types::GraphFactsScanDiagnostic>>);

impl Gaps {
    fn push(&self, gap: octocode_engine::types::GraphFactsScanDiagnostic) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(gap);
    }
}

/// The skip of a file whose source correspondence passed its bounds
/// ([`capture::capture`]'s `message`).
fn declined(
    relative_path: &str,
    message: String,
) -> octocode_engine::types::GraphFactsScanDiagnostic {
    octocode_engine::types::GraphFactsScanDiagnostic {
        relative_path: relative_path.to_owned(),
        code: octocode_engine::fact_scan::SOURCE_BUDGET_EXCEEDED.to_owned(),
        message,
    }
}

/// Name the files the map left out (past the per-file budget, or failing
/// their secret scan) among the scan's gaps, sorted with them, and count
/// every gap as a skipped file.
fn name_gaps(
    facts: &mut GraphFactsMappedScan<Kept>,
    gaps: Vec<octocode_engine::types::GraphFactsScanDiagnostic>,
) {
    facts.skipped.extend(gaps);
    facts
        .skipped
        .sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    // One file can be named by two stages (the scan's and the capture's
    // per-file budget): it is one skipped file.
    let mut files = facts
        .skipped
        .iter()
        .map(|skip| skip.relative_path.as_str())
        .collect::<Vec<_>>();
    files.dedup();
    facts.files_skipped = octocode_engine::num::saturating_u32(files.len());
}

/// One scanned file's turn: capture its source, keep its facts (in memory or
/// as the checkpoint's artifact) and charge the snapshot's admission.
/// `None` when the file is a named gap (past the per-file budget, or failing
/// its secret scan); the whole-snapshot budget still ends the scan.
fn keep_file(
    view: &octocode_engine::fact_scan::GraphFactsView<'_>,
    cancel: &dyn CancellationCheck,
    job: Option<&checkpoint::Job<'_>>,
    admission: &super::scope::Admission<'_>,
    gaps: &Gaps,
    source_map: octocode_engine::text::analysis::MapLimits,
) -> Result<Option<Kept>, ToolError> {
    // A file whose correspondence no build could hold is the scan's named
    // gap (`None`); the build budget still ends the scan.
    let source = match capture::capture(view, cancel, source_map) {
        Ok(source) => source,
        Err(error) if error.code == ErrorCode::ScopeTooBroad => {
            gaps.push(declined(view.relative_path, error.message));
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let source_bytes = source.bytes();
    // The facts' encoded size: their artifact's, else their
    // JSON's, which the artifact encodes.
    let (kept, fact_bytes) = match job {
        Some(job) => match job.keep(view, source)? {
            Ok(kept) => kept,
            Err(unscanned) => {
                gaps.push(octocode_engine::fact_scan::panicked(
                    view.relative_path.to_owned(),
                    &unscanned,
                ));
                return Ok(None);
            }
        },
        None => {
            let marks = super::super::classify::SourceMarks::of(view.relative_path, view.content);
            let fact_bytes = octocode_engine::json_size::bytes_or(view.facts, usize::MAX);
            (
                Kept::Memory(Box::new((view.to_entry(), marks, source))),
                fact_bytes,
            )
        }
    };
    admission.admit(
        view.facts.declarations.len(),
        super::super::limits::build_charge(fact_bytes, view.facts.declarations.len(), source_bytes),
    )?;
    Ok(Some(kept))
}

pub(super) fn extract<'a>(
    scan: &GraphScan,
    root: &Path,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    turn: &super::turn::ExtractionTurn<'_>,
    registry: RegistryView<'_>,
) -> Result<Extracted<'a>, ToolError> {
    // The walk runs under the request alone; parsing and capture under the
    // turn, which may end extraction before the request does.
    let request = turn.request();
    let cancel: &dyn CancellationCheck = turn;
    let options = scan_options(scan, root);
    let admission_of_walk = || super::super::walk_scope::build_admission(paths, root);
    let discovery = admission_of_walk();
    let admit = |path: &Path, file_type| discovery.admit(path, file_type, |_| true);
    let capture_error = std::sync::Mutex::new(None);
    let walk_stop = || request.check().is_err();
    let walk_error = |error| ToolError::graph_scan(&error, request);
    let stop = || {
        cancel.check().is_err()
            || capture_error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some()
    };
    let scan_error = |error| ToolError::graph_scan(&error, cancel);
    let walk = octocode_engine::fact_scan::walk_graph_fact_files_with_registry(
        &options, &admit, &walk_stop, registry,
    )
    .map_err(walk_error)?;
    // Its own admission: what this walk withholds was counted by the one above.
    let markup_discovery = admission_of_walk();
    let markup = octocode_engine::fact_scan::walk_script_markup_files(
        &options,
        &|path: &Path, file_type| markup_discovery.admit(path, file_type, |_| true),
        &walk_stop,
    )
    .map_err(walk_error)?
    .entries;
    turn.record_walk(walk.entries.len());
    let job = checkpoint::Job::open(
        scan,
        root,
        &walk.entries,
        paths,
        security,
        registry,
        request,
    )?;
    let parsing = crate::tools::ast_graph::progress::Parsing::new(walk.entries.len());
    let admission = super::scope::Admission::new(scan, root, &walk.entries);
    let source_map = scan.limits.source_map();
    let gaps = Gaps::default();
    let reused_files = std::sync::atomic::AtomicUsize::new(0);
    let reuse = |context: &octocode_engine::fact_scan::GraphFactsReuseContext<'_>| {
        job.as_ref().and_then(|job| job.load(context))
    };
    // A checkpointed build reuses its own verified artifacts; one without
    // extracts the selected sources afresh.
    let source = match &job {
        Some(_) => FactsSource::CachedGraph(&reuse),
        None => FactsSource::Graph,
    };
    let facts = self::scan(
        options,
        walk,
        source,
        &|view: &octocode_engine::fact_scan::GraphFactsView<'_>| {
            if view.reused {
                reused_files.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            parsing.file_done();
            match keep_file(view, cancel, job.as_ref(), &admission, &gaps, source_map) {
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
        source_map,
    );
    if let Some(error) = capture_error
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
    {
        return Err(error);
    }
    let mut facts = facts.map_err(scan_error)?;
    parsing.finish();
    name_gaps(
        &mut facts,
        gaps.0.into_inner().unwrap_or_else(PoisonError::into_inner),
    );
    let reused_files = reused_files.load(std::sync::atomic::Ordering::Relaxed);
    Ok(Extracted {
        facts,
        withheld: discovery.skipped().withheld,
        job,
        reused_files,
        markup,
        tally: admission.tally(),
    })
}
