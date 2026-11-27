use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::language::AgLanguage;
use super::octo::{CompileError, OctoCompiledMatcher, compile_matcher};
use super::query::{StructuralQuery, invalid_query_diagnostic, invalid_query_explanation};
use super::types::{
    KeepLine, StructuralDemand, StructuralDiagnostic, StructuralSearchFilesDetailedResult,
    StructuralSearchFilesOptions, WalkErrors, WalkSkipped,
};
use crate::digest::{ListedFiles, stamp_of};
use crate::error::{Error, Result};
use crate::search::walk::{Prune, Stop};
use crate::signatures::languages;
use crate::stamp::SourceStamp;
use crate::text::extension_of;

pub(super) mod anchors;
mod counts;
mod file_scan;
mod kept;
mod rebuild;
mod shared;
mod walk;

use anchors::AnchorFilter;
use counts::ScanCounts;
use file_scan::{FileOutcome, FileScan};
pub use rebuild::{Rebuilt, file_matches};
pub use shared::{TreeShare, Walk, WalkRecord};
use walk::{Walked, check_root_exists, collect_files};

/// Test helper: [`search_files`] with no path policy and each file's own
/// extension.
#[cfg(test)]
pub(crate) fn search_files_detailed(
    options: StructuralSearchFilesOptions,
) -> Result<StructuralSearchFilesDetailedResult> {
    search_files(
        options,
        StructuralDemand::Matches,
        &|_, _| true,
        &|path| extension_for_path(path).unwrap_or_default(),
        &|| false,
        None,
    )
}

#[cfg(test)]
thread_local! {
    /// Set by [`search_files_unfiltered`]: the scan on this thread runs with
    /// no literal prefilter and its matchers re-parse every macro body.
    static UNFILTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test helper: [`search_files_detailed`] with no literal anywhere: no
/// prefilter (every candidate is parsed) and no macro-body gate (every
/// body is re-parsed). The reference the literal gates must agree with.
#[cfg(test)]
pub(crate) fn search_files_unfiltered(
    options: StructuralSearchFilesOptions,
) -> Result<StructuralSearchFilesDetailedResult> {
    UNFILTERED.with(|unfiltered| unfiltered.set(true));
    let result = search_files_detailed(options);
    UNFILTERED.with(|unfiltered| unfiltered.set(false));
    result
}

/// Whether this thread runs [`search_files_unfiltered`].
#[cfg(test)]
pub(super) fn unfiltered() -> bool {
    UNFILTERED.with(std::cell::Cell::get)
}

/// Detailed structural search with caller-owned descendant policy and a
/// caller-selected parser for ambiguous paths. `admit` decides each walked
/// entry, by its listing type, before candidate accounting; it also sees every
/// directory and every link the walk does not follow, whose verdict decides
/// nothing. Before a candidate's prefilter and source reads it decides again,
/// by the type the entry has then (`None` for the root, or an entry that
/// cannot be read). File paths remain
/// unchanged in diagnostics and match IDs. `stop` cancels the walk and the scan
/// ([`crate::error::ErrorKind::Cancelled`]). `demand` decides what each
/// file keeps of its matches ([`StructuralDemand`]); counts and coverage
/// are the same under every demand.
///
/// An unset `max_files` caps nothing but the time: the scan evaluates
/// candidates in chunks and stops at the first chunk boundary past
/// [`crate::scan_budget::SCAN_TIME_BUDGET`], reporting where it resumes
/// ([`StructuralSearchFilesDetailedResult::scan_resume`]). An explicit
/// `max_files` is the caller's window: never cut by time, and resumed after
/// it like any cut. `keep_line` keeps only the matches whose file and
/// 1-based start line it admits (the caller counts what it refuses).
pub fn search_files(
    options: StructuralSearchFilesOptions,
    demand: StructuralDemand<'_>,
    admit: &Admit<'_>,
    select_extension: &(dyn Fn(&Path) -> String + Sync),
    stop: &Stop<'_>,
    keep_line: Option<&KeepLine>,
) -> Result<StructuralSearchFilesDetailedResult> {
    search_files_shared(
        options,
        demand,
        (admit, keep_line),
        select_extension,
        stop,
        ScanShare::default(),
    )
}

/// What a scan shares with the scans of the same scope that run beside it
/// ([`search_files_shared`]).
#[derive(Clone, Copy, Default)]
pub struct ScanShare<'a> {
    /// How it walks: afresh, recording its walk, or replaying a recorded
    /// walk of the same walk inputs.
    pub walk: Walk<'a>,
    /// The trees of the sources the scans beside it parse.
    pub trees: Option<&'a TreeShare>,
}

/// [`search_files`] sharing `share`: a replayed walk calls `admit` as a
/// walk of the same entries would, and a shared tree is the parse a scan
/// of its own would make of the same text, so the result is the one
/// [`search_files`] gives over the same files.
pub fn search_files_shared(
    options: StructuralSearchFilesOptions,
    demand: StructuralDemand<'_>,
    (admit, keep_line): (&Admit<'_>, Option<&KeepLine>),
    select_extension: &(dyn Fn(&Path) -> String + Sync),
    stop: &Stop<'_>,
    share: ScanShare<'_>,
) -> Result<StructuralSearchFilesDetailedResult> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_files_with_budget(
            options,
            demand,
            (admit, keep_line),
            select_extension,
            (stop, share),
            Some(crate::scan_budget::SCAN_TIME_BUDGET),
        )
    }))
    .unwrap_or_else(|_| {
        Err(Error::new(
            "structural detailed file search failed on pathological input",
        ))
    })
}

/// Which walked entries a search reads, by path and entry type when known.
type Admit<'a> = dyn Fn(&Path, Option<std::fs::FileType>) -> bool + Sync + 'a;

/// [`search_files`] with its time budget (`None` evaluates every candidate).
pub(crate) fn search_files_with_budget(
    options: StructuralSearchFilesOptions,
    demand: StructuralDemand<'_>,
    (admit, keep_line): (&Admit<'_>, Option<&KeepLine>),
    select_extension: &(dyn Fn(&Path) -> String + Sync),
    (stop, share): (&Stop<'_>, ScanShare<'_>),
    budget: Option<std::time::Duration>,
) -> Result<StructuralSearchFilesDetailedResult> {
    let StructuralSearchFilesOptions {
        path,
        pattern,
        rule,
        include,
        exclude,
        default_exclude_dir,
        ignore,
        max_depth,
        max_files,
        skip_files,
        max_file_bytes,
        judge,
        global_ignore,
        kept,
    } = options;
    let pattern_ref = pattern.as_deref();
    let rule_ref = rule.as_ref();
    let query = match StructuralQuery::new(pattern_ref, rule_ref) {
        Ok(query) => query,
        Err(message) => return Ok(invalid_query_result(path, pattern_ref, rule_ref, message)),
    };

    let root = PathBuf::from(&path);
    if !admit(&root, None) {
        return Err(Error::new(
            "Structural search root is denied by path policy",
        ));
    }
    check_root_exists(&root)?;
    let timed = max_files.is_none();
    let max_files = max_files.map_or(usize::MAX, |n| n as usize);
    let max_file_bytes = max_file_bytes
        .map(|n| n as u64)
        .unwrap_or(crate::security::MAX_CONTENT_SIZE as u64);
    let prefilter = query.prefilter();
    #[cfg(test)]
    let prefilter = if unfiltered() {
        super::query::Prefilter::None
    } else {
        prefilter
    };
    let query_explanation = query.explanation_with_prefilter(&prefilter);

    let globs = crate::search::glob::compile(
        &root,
        &include.unwrap_or_default(),
        &exclude.unwrap_or_default(),
    )?;
    let (mut candidate_files, walk_errors, walk_skipped) = collect_files(
        &root,
        &Walked {
            globs: &globs,
            defaults: default_exclude_dir
                .map(|names| Prune::new(names).at_with(&root, global_ignore)),
            global_ignore,
            ignore,
            max_depth,
            max_files: max_files.saturating_add(1),
        },
        (admit, share.walk),
        stop,
    )?;
    let scan_truncated = candidate_files.len() > max_files;
    candidate_files.truncate(max_files);
    // Walk order is stable, so the leading candidates an earlier window
    // evaluated are exactly these.
    let skip = skip_files
        .map_or(0, |n| n as usize)
        .min(candidate_files.len());
    // A paged window the one before ended inside the last of them reads
    // that one again: its pin still covers it, as its listing did.
    let reread = usize::from(
        skip > 0
            && matches!(
                demand,
                StructuralDemand::Paged(super::types::PagedWindow {
                    resumes_inside: true,
                    ..
                })
            ),
    );
    let start = skip - reread;
    // What this window's resume query pinned is what those candidates are now.
    let mut listed = ListedFiles::default();
    let listed_before = (skip > 0).then(|| {
        let stamps = candidate_files[..skip]
            .par_iter()
            .map(|file_path| stamp_of(file_path))
            .collect::<Vec<_>>();
        for (file_path, stamp) in candidate_files.iter().zip(stamps) {
            listed.add(file_path, stamp);
        }
        listed.hex()
    });
    let earlier_files = candidate_files.drain(..start).collect::<Vec<_>>();
    // Compile one matcher per extension up front (cheap, serial), then read,
    // prefilter, parse and match files in parallel. `collect` keeps candidate
    // order, and the serial fold below rebuilds the same counters.
    let parse_budget = crate::structural::parse_budget();
    // A line scope's verdicts read other files: only a scan without one
    // keeps each file's outcome.
    let kept = kept.filter(|_| keep_line.is_none()).and_then(|caller| {
        kept::Kept::new(
            &caller,
            &demand,
            &kept::Inputs {
                pattern: pattern_ref,
                rule: rule_ref,
                max_file_bytes,
                parse_budget,
                judged: judge.is_some(),
            },
        )
    });
    let scan = FileScan {
        admit,
        stop,
        select_extension,
        anchors: AnchorFilter::new(&prefilter).map_err(Error::new)?,
        matchers: compile_matchers(&candidate_files, select_extension, &query),
        max_file_bytes,
        parse_budget,
        demand,
        keep_line,
        judge: judge.as_ref(),
        kept,
        trees: share.trees,
    };
    let pacer = crate::scan_budget::ScanPacer::new(
        budget.filter(|_| timed),
        crate::scan_budget::WalkClock::default(),
    );
    let ScannedChunks {
        outcomes,
        resume: mut scan_resume,
        page_stop: page_filled,
        split: page_split,
    } = scan_in_chunks(&scan, &candidate_files, (start, skip), pacer)?;
    let resumed_inside = reread > 0
        && outcomes
            .first()
            .is_some_and(|(_, outcome)| outcome.is_some());
    let scan_truncated = scan_truncated || scan_resume.is_some();
    // Every cut resumes where it stopped: the walk cap of an unset
    // `max_files`, and a caller's file window.
    if scan_resume.is_none() && scan_truncated {
        scan_resume = Some(u32::try_from(start + candidate_files.len()).unwrap_or(u32::MAX));
    }
    // The reread candidate is in the pin already.
    let listed_through = scan_resume.map(|_| {
        for (file_path, (stamp, _)) in candidate_files.iter().zip(&outcomes).skip(reread) {
            listed.add(file_path, *stamp);
        }
        listed.hex()
    });

    let mut counts = ScanCounts {
        walk_errors: walk_errors.skipped,
        ..ScanCounts::default()
    };
    let mut files = Vec::with_capacity(outcomes.len());
    for (tally, file) in outcomes.into_iter().filter_map(|(_, outcome)| outcome) {
        counts.record(tally);
        files.push(file);
    }
    let warnings = counts.warnings(&prefilter, query.is_rule(), max_file_bytes);
    let status = counts.status(&files);
    let diagnostics = files
        .iter()
        .filter(|file| file.status == "truncated")
        .flat_map(|file| file.diagnostics.iter().cloned())
        .collect();
    Ok(StructuralSearchFilesDetailedResult {
        scan_truncated,
        page_filled,
        page_split,
        resumed_inside,
        scan_resume,
        listed_before,
        listed_through,
        earlier_files,
        files,
        total_matches: counts.total_matches,
        parsed_files: counts.parsed_files,
        skipped_by_pre_filter: counts.skipped_by_pre_filter,
        skipped_unsupported: counts.skipped_unsupported,
        skipped_unreadable: counts.skipped_unreadable,
        skipped_large: counts.skipped_large,
        walk_errors,
        walk_skipped,
        status: status.to_owned(),
        query: query_explanation,
        diagnostics,
        warnings,
    })
}

/// What [`scan_in_chunks`] evaluated: each candidate's stamp and outcome,
/// where the scan resumes, whether it stopped at a page start (else at its
/// time), and, for a page start inside its last file, how many of that file's
/// matches its pages hold.
struct ScannedChunks {
    outcomes: Vec<(Option<SourceStamp>, FileOutcome)>,
    resume: Option<u32>,
    page_stop: bool,
    split: Option<u32>,
}

/// Evaluates `candidates` (from walk position `start`; the windows before
/// read `skip`) in the pacer's chunks, in order, and stops at the first chunk
/// boundary the pacer calls spent, or, under [`StructuralDemand::Paged`], at
/// the window's page start ([`window_end`]). The outcomes are those of the
/// candidates the window keeps; the resume offset is set only when
/// candidates remain, and so is the page stop.
fn scan_in_chunks(
    scan: &FileScan<'_>,
    candidates: &[PathBuf],
    (start, skip): (usize, usize),
    mut pacer: crate::scan_budget::ScanPacer,
) -> Result<ScannedChunks> {
    let page_starts = match scan.demand {
        StructuralDemand::Paged(window) => Some(window.page_starts),
        _ => None,
    };
    // A paged scan's chunks start at the threads the call computes on, so
    // every worker is busy, and double: the window evaluates at most about
    // twice the files it keeps.
    let mut page_chunk = page_starts.map(|_| crate::cpu::host().share().max(1));
    // Every page start met so far: (position from `start`, match index), and
    // the last position of a file holding a match.
    let mut starts = Vec::new();
    let mut last_matched = None;
    let mut outcomes = Vec::with_capacity(candidates.len());
    let at = |position: usize| Some(u32::try_from(start + position).unwrap_or(u32::MAX));
    let mut rest = candidates;
    while !rest.is_empty() {
        if pacer.spent() {
            // A window that met a page start ends at its last one, its pages
            // whole; else this is the time cut.
            return Ok(match starts.last() {
                Some(&page) => end_at_page(outcomes, page, start),
                None => ScannedChunks {
                    resume: at(outcomes.len()),
                    outcomes,
                    page_stop: false,
                    split: None,
                },
            });
        }
        let size =
            page_chunk.map_or_else(|| pacer.next_chunk(), |chunk| chunk.min(pacer.next_chunk()));
        let (chunk, tail) = rest.split_at(size.min(rest.len()));
        rest = tail;
        // Matching recurses with the pattern, so the chunk runs on the
        // large-stack pool and each file's matcher runs inline there
        // ([`super::octo::on_deep_stack`]); without a pool, here, and each
        // matcher refuses on its own.
        let run = || {
            chunk
                .par_iter()
                .map(|file_path| scan.file(file_path))
                .collect::<Result<Vec<_>>>()
        };
        let scanned = super::octo::on_deep_stack(run).unwrap_or_else(run)?;
        let found = matches!(scan.demand, StructuralDemand::Exists)
            && scanned.iter().any(|(_, outcome)| {
                outcome
                    .as_ref()
                    .is_some_and(|(_, file)| !file.matches.is_empty())
            });
        if let Some(page_starts) = page_starts {
            let files = scanned
                .iter()
                .enumerate()
                .filter_map(|(at, (_, outcome))| {
                    outcome
                        .as_ref()
                        .map(|(_, file)| (outcomes.len() + at, file))
                })
                .collect::<Vec<_>>();
            starts.extend(page_starts(&files));
        }
        if let Some(at) = scanned.iter().rposition(|(_, outcome)| {
            outcome
                .as_ref()
                .is_some_and(|(_, file)| !file.matches.is_empty())
        }) {
            last_matched = Some(outcomes.len() + at);
        }
        outcomes.extend(scanned);
        pacer.record(chunk.len());
        // The window ends at its page start once a match lies past it in
        // another file, so the window after it is never empty; with none, it
        // ends with the scope, whole.
        if let Some(&page) = starts
            .iter()
            .find(|(position, _)| *position >= window_end(skip))
            && (page.1 == 0 || last_matched > Some(page.0))
        {
            return Ok(end_at_page(outcomes, page, start));
        }
        if found {
            let resume = if rest.is_empty() {
                None
            } else {
                at(outcomes.len())
            };
            return Ok(ScannedChunks {
                outcomes,
                resume,
                page_stop: false,
                split: None,
            });
        }
        page_chunk = page_chunk.map(|chunk| chunk.saturating_mul(2));
    }
    Ok(ScannedChunks {
        outcomes,
        resume: None,
        page_stop: false,
        split: None,
    })
}

/// The window ending at the page start `(position, index)`: the files before
/// its file, and that file too when the page starts inside it (the next
/// window reads it again, its first `index` matches listed here); the
/// window after resumes at the next candidate. The files past it were
/// evaluated with its chunk but are the next window's: it reads them again
/// (from the kept outcomes), so dropping them here loses nothing.
fn end_at_page(
    mut outcomes: Vec<(Option<SourceStamp>, FileOutcome)>,
    (position, index): (usize, usize),
    start: usize,
) -> ScannedChunks {
    let (kept, split) = if index > 0 {
        (position + 1, Some(u32::try_from(index).unwrap_or(u32::MAX)))
    } else {
        (position, None)
    };
    #[cfg_attr(
        octocode_clippy_ratchet,
        expect(
            clippy::disallowed_methods,
            reason = "the dropped outcomes are the next window's, which evaluates them again"
        )
    )]
    outcomes.truncate(kept);
    ScannedChunks {
        resume: Some(u32::try_from(start + kept).unwrap_or(u32::MAX)),
        outcomes,
        page_stop: true,
        split,
    }
}

/// The fewest candidates, from its first, before the page start a paged
/// window ends at, when the windows before it read `skip`: no fewer than
/// they did, so the first window is its first page, each later one passes
/// the file it may read again and at least doubles what was read, and a walk
/// through every window takes a number of windows logarithmic in the scope.
/// It is a fact of the files and the pages, not of the chunks: the same
/// window ends at the same page start however its chunks were cut, so a
/// continuation rescans the window its first page did.
fn window_end(skip: usize) -> usize {
    skip
}

type CompiledMatchers = BTreeMap<String, std::result::Result<OctoCompiledMatcher, CompileError>>;

fn invalid_query_result(
    path: String,
    pattern: Option<&str>,
    rule: Option<&serde_json::Value>,
    message: String,
) -> StructuralSearchFilesDetailedResult {
    let diagnostic = invalid_query_diagnostic(message.clone()).with_path(path);
    StructuralSearchFilesDetailedResult {
        scan_truncated: false,
        page_filled: false,
        page_split: None,
        resumed_inside: false,
        scan_resume: None,
        listed_before: None,
        listed_through: None,
        earlier_files: Vec::new(),
        files: Vec::new(),
        total_matches: 0,
        parsed_files: 0,
        skipped_by_pre_filter: 0,
        skipped_unsupported: 0,
        skipped_unreadable: 0,
        skipped_large: 0,
        walk_errors: WalkErrors::default(),
        walk_skipped: WalkSkipped::default(),
        status: "parserFailed".to_owned(),
        query: invalid_query_explanation(pattern, rule, &message),
        diagnostics: vec![diagnostic],
        warnings: Vec::new(),
    }
}

fn compile_matchers(
    candidate_files: &[PathBuf],
    select_extension: &(dyn Fn(&Path) -> String + Sync),
    query: &StructuralQuery,
) -> CompiledMatchers {
    let mut matchers = CompiledMatchers::new();
    for file_path in candidate_files {
        let ext = select_extension(file_path);
        if matchers.contains_key(&ext) {
            continue;
        }
        if let Some(entry) = languages::find_entry(&ext) {
            let lang = AgLanguage::new(entry);
            let compiled = compile_matcher(&lang, query);
            matchers.insert(ext, compiled);
        }
    }
    matchers
}

/// The diagnostic of a file whose extension selects no grammar, naming the
/// extension it has (or that it has none).
pub(super) fn unsupported_language(file_path: &str) -> StructuralDiagnostic {
    let extension = match extension_of(file_path, true, "").as_str() {
        "" => "files with no extension".to_owned(),
        extension => format!(".{extension} files"),
    };
    StructuralDiagnostic::new(
        "structural.language.unsupported",
        "warning",
        "parse",
        format!("Structural search has no grammar for {extension}."),
    )
    .with_path(file_path)
}

#[cfg(test)]
fn extension_for_path(path: &Path) -> Option<String> {
    let extension = extension_of(&path.to_string_lossy(), true, "");
    (!extension.is_empty()).then_some(extension)
}
