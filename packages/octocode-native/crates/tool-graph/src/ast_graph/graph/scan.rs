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
    memory: &octocode_engine::fact_scan::ScanMemory<'_>,
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
        Some(memory),
    )?;
    let mut out = GraphFactsMappedScan {
        kept: result.kept,
        parsed: result.parsed,
        passed_over: result.passed_over,
        skipped: result.skipped,
        decoded: result.decoded,
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
    pub(super) withheld: octocode_runtime_base::policy::discovery::Withheld,
    pub(super) job: Option<checkpoint::Job<'a>>,
    pub(super) reused_files: usize,
    /// The markup files the scan's walk leaves out: no graph node holds them.
    pub(super) markup: Vec<octocode_engine::types::FileSystemEntry>,
    /// What the kept files certainly project.
    pub(super) tally: super::super::limits::Tally,
    /// The directories the walk was denied, relative to the scan root.
    pub(super) denied_dirs: Vec<String>,
    /// Each kept file a read hides a value of, with what the read replaced.
    pub(super) hidden_spans: HiddenSpans,
}

/// What a read replaced of each kept file it hides a value of, by
/// position: `[sourceStart, sourceEnd, shownStart, shownEnd]` per replaced
/// run, the source bytes the snapshot's ranges count and the bytes of the
/// text a read shows in their place.
pub(super) type HiddenSpans = std::collections::BTreeMap<String, Vec<[u32; 4]>>;

/// The scan options of a graph build of `root`: the one default prune (a
/// git-tracked `build/` is scanned, as every local tool walks it) unless
/// `defaultExcludes:false`, the ignore policy `includeIgnored` and
/// `includeHidden` name; the credential stores are the path policy's.
fn scan_options(scan: &GraphScan, root: &Path, paths: &PathPolicy) -> GraphFactsScanOptions {
    GraphFactsScanOptions {
        path: root.to_string_lossy().into_owned(),
        default_exclude_dir: Some(octocode_runtime_base::policy::prune::walk_names(
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
            .then(|| octocode_tools_shared::ast_rule::language_globs(&scan.language_globs)),
        ignore: scan.walk.ignore,
        global_ignore: paths.global_ignore(),
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

/// The replaced runs of the kept files a read hides a value of
/// ([`HiddenSpans`]), gathered from the files in flight.
#[derive(Default)]
struct Hidden(std::sync::Mutex<HiddenSpans>);

impl Hidden {
    fn record(&self, file: &str, edits: &octocode_engine::security::Edits) {
        let at = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
        let runs = edits
            .redacted()
            .zip(edits.outputs())
            .map(|(source, shown)| {
                [
                    at(source.start),
                    at(source.end),
                    at(shown.start),
                    at(shown.end),
                ]
            })
            .collect();
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(normalize(file), runs);
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
/// every gap as a skipped file. `unreadable` entries the walk could not
/// read (a directory it may not enter: it counts them without naming them)
/// count as skipped too, by their number (the larger of the walk's two
/// counts, which can tally one entry twice), so the snapshot's answers are
/// partial.
fn name_gaps(
    facts: &mut GraphFactsMappedScan<Kept>,
    gaps: Vec<octocode_engine::types::GraphFactsScanDiagnostic>,
    unreadable: u32,
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
    facts.files_skipped =
        octocode_engine::num::saturating_u32(files.len()).saturating_add(unreadable);
}

/// One scanned file's turn: capture its source, keep its facts (in memory or
/// as the checkpoint's artifact) and charge the snapshot's admission.
/// `None` when the file is a named gap (past the per-file budget, or failing
/// its secret scan); the whole-snapshot budget still ends the scan.
fn keep_file(
    view: &octocode_engine::fact_scan::GraphFactsView<'_>,
    cancel: &dyn CancellationCheck,
    (job, judging): (Option<&checkpoint::Job<'_>>, (&ContentSecurity, &Path)),
    admission: &super::scope::Admission<'_>,
    (gaps, hidden): (&Gaps, &Hidden),
    source_map: octocode_engine::text::analysis::MapLimits,
) -> Result<Option<Kept>, ToolError> {
    // One redaction decision per source, made over its whole text: the
    // facts kept, linked and stored are the ones that judged source shows.
    let (security, root) = judging;
    let path = if root.is_file() {
        root.to_path_buf()
    } else {
        root.join(view.relative_path)
    };
    let judged = match checkpoint::judged_source(security, view.content, &path) {
        Ok(judged) => judged,
        Err(unscanned) => {
            gaps.push(octocode_engine::fact_scan::panicked(
                view.relative_path.to_owned(),
                &unscanned,
            ));
            return Ok(None);
        }
    };
    let holds_secret = judged.is_redacted();
    let replaced = judged.redaction().map(|(_, edits)| edits.clone());
    let shown = match checkpoint::judged_facts(view.facts, &judged) {
        Ok(shown) => shown,
        Err(error) => {
            gaps.push(declined(
                view.relative_path,
                format!(
                    "graph facts could not be rewritten as the redacted source shows them: {error}"
                ),
            ));
            return Ok(None);
        }
    };
    drop(judged);
    let shown_view;
    let view = match &shown {
        Some(facts) => {
            shown_view = octocode_engine::fact_scan::GraphFactsView { facts, ..*view };
            &shown_view
        }
        None => view,
    };
    // A file whose correspondence no build could hold is the scan's named
    // gap (`None`); one that cannot be held beside the files kept so far is
    // the scope's refusal; one refused only because files in flight held
    // the memory runs again alone (the engine retries it, and this turn is
    // not its outcome).
    let source = match capture::capture(view, cancel, source_map) {
        Ok(source) => source,
        Err(capture::Uncaptured::Limit { name, exceeded }) => {
            if let Some(hold) = view.memory
                && name == "bytes"
            {
                if hold.contended() {
                    return Ok(None);
                }
                // Beside kept files: the scope's refusal when the file would
                // fit alone, its skip when no build could hold it.
                let memory = hold.memory();
                let alone = octocode_engine::text::analysis::MapLimits {
                    bytes: memory.budget().saturating_sub(hold.base()),
                    ..source_map
                };
                if memory.committed() > 0 && capture::fits_alone(view, cancel, alone) {
                    memory.refuse(view.relative_path, hold.needed());
                    return Ok(None);
                }
            }
            gaps.push(declined(
                view.relative_path,
                octocode_engine::fact_scan::over_budget_message(&exceeded),
            ));
            return Ok(None);
        }
        Err(capture::Uncaptured::Failed(error)) => return Err(error),
    };
    let source_bytes = source.bytes();
    // The facts' encoded size: their artifact's, else their
    // JSON's, which the artifact encodes.
    let (kept, fact_bytes) = match job {
        Some(job) => job.keep(view, source, holds_secret)?,
        None => {
            let marks = super::super::classify::SourceMarks::of(view.relative_path, view.content)
                .hiding(holds_secret);
            let fact_bytes = octocode_engine::json_size::bytes_or(view.facts, usize::MAX);
            (
                Kept::Memory(Box::new((view.to_entry(), marks, source))),
                fact_bytes,
            )
        }
    };
    let charge =
        super::super::limits::build_charge(fact_bytes, view.facts.declarations.len(), source_bytes);
    admission.admit(view.facts.declarations.len(), charge)?;
    if let Some(hold) = view.memory {
        hold.memory().commit(charge);
    }
    if let Some(edits) = &replaced {
        hidden.record(view.relative_path, edits);
    }
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
    let options = scan_options(scan, root, paths);
    let admission_of_walk = || super::super::walk_scope::build_admission(paths, root);
    let discovery = admission_of_walk();
    let admit = |path: &Path, file_type| discovery.admit(path, file_type, |_| true);
    let capture_error = std::sync::Mutex::new(None);
    // The build's memory, shared by the files in flight: each file first
    // fits what keeping its facts costs beside what is kept, then draws its
    // source correspondence from what is left.
    // What a file holds before its correspondence is priced by its
    // declarations ([`super::super::limits::DECLARATION_BUILD_BYTES`] each),
    // which costs no pass over its facts; their encoded size joins the
    // charge when the file is kept.
    let base = |facts: &octocode_engine::graph::GraphFactsDocument| {
        super::super::limits::build_charge(0, facts.declarations.len(), 0)
    };
    let memory = octocode_engine::fact_scan::ScanMemory::new(scan.limits.build_bytes, &base);
    let walk_stop = || request.check().is_err();
    let walk_error = |error| ToolError::graph_scan(&error, request);
    let stop = || {
        cancel.check().is_err()
            || memory.refused().is_some()
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
    let unreadable = walk.permission_denied.max(walk.skipped);
    let denied_dirs = walk
        .denied_dirs
        .iter()
        .filter_map(|dir| Path::new(dir).strip_prefix(root).ok())
        .map(octocode_engine::text::slash_path)
        .collect::<Vec<_>>();
    turn.record_walk(walk.entries.len());
    let job = checkpoint::Job::open(scan, root, &walk.entries, paths, registry, request)?;
    let parsing = crate::ast_graph::progress::Parsing::new(walk.entries.len());
    let admission = super::scope::Admission::new(scan, root, &walk.entries);
    let source_map = scan.limits.source_map();
    let gaps = Gaps::default();
    let hidden = Hidden::default();
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
            let judging = (job.as_ref(), (security, root));
            let kept = match keep_file(
                view,
                cancel,
                judging,
                &admission,
                (&gaps, &hidden),
                source_map,
            ) {
                Ok(kept) => kept,
                Err(error) => {
                    capture_error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get_or_insert(error);
                    None
                }
            };
            // A file the engine runs again alone is counted on that turn.
            if !view
                .memory
                .is_some_and(octocode_engine::fact_scan::FileHold::contended)
            {
                if view.reused {
                    reused_files.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                parsing.file_done();
            }
            kept
        },
        &stop,
        registry,
        source_map,
        &memory,
    );
    if let Some(refusal) = memory.refused() {
        return Err(admission.refuse_build(&refusal));
    }
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
        unreadable,
    );
    let reused_files = reused_files.load(std::sync::atomic::Ordering::Relaxed);
    Ok(Extracted {
        denied_dirs,
        hidden_spans: hidden
            .0
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner),
        facts,
        withheld: discovery.skipped().withheld,
        job,
        reused_files,
        markup,
        tally: admission.tally(),
    })
}
