use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::language::AgLanguage;
use super::octo::{CompileError, OctoCompiledMatcher, compile_matcher};
use super::query::{StructuralQuery, invalid_query_diagnostic, invalid_query_explanation};
use super::types::{
    StructuralDemand, StructuralDiagnostic, StructuralSearchFilesDetailedResult,
    StructuralSearchFilesOptions, WalkErrors,
};
use crate::error::{Error, Result as EngineResult};
use crate::search::walk::{Prune, Stop};
use crate::signatures::languages;
use crate::text::file_extension::extension_of;

mod anchors;
mod counts;
mod file_scan;
mod rebuild;
mod walk;

use anchors::AnchorFilter;
use counts::ScanCounts;
use file_scan::FileScan;
pub use rebuild::{Rebuilt, file_matches};
use walk::{Walked, check_root_exists, collect_files};

/// Test helper: [`search_files`] with no path policy and each file's own
/// extension.
#[cfg(test)]
pub(crate) fn search_files_detailed(
    options: StructuralSearchFilesOptions,
) -> EngineResult<StructuralSearchFilesDetailedResult> {
    search_files(
        options,
        StructuralDemand::Matches,
        &|_| true,
        &|path| extension_for_path(path).unwrap_or_default(),
        &|| false,
    )
}

/// Detailed structural search with caller-owned descendant policy and a
/// caller-selected parser for ambiguous paths. `admit` decides each walked
/// entry before candidate accounting, literal prefilter reads, metadata
/// reads, and source reads; it also sees every directory and every link the
/// walk does not follow, whose verdict decides nothing. File paths remain
/// unchanged in diagnostics and match IDs. `stop` cancels the walk and the scan
/// ([`crate::error::ErrorKind::Cancelled`]). `demand` decides what each
/// file keeps of its matches ([`StructuralDemand`]); counts and coverage
/// are the same under every demand.
///
/// An unset `max_files` caps nothing but the time: the scan evaluates
/// candidates in chunks and stops at the first chunk boundary past
/// [`crate::scan_budget::SCAN_TIME_BUDGET`], reporting where it resumes
/// ([`StructuralSearchFilesDetailedResult::scan_resume`]). An explicit
/// `max_files` is the caller's window and is never cut by time.
pub fn search_files(
    options: StructuralSearchFilesOptions,
    demand: StructuralDemand<'_>,
    admit: &(dyn Fn(&Path) -> bool + Sync),
    select_extension: &(dyn Fn(&Path) -> String + Sync),
    stop: &Stop<'_>,
) -> EngineResult<StructuralSearchFilesDetailedResult> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_files_with_budget(
            options,
            demand,
            admit,
            select_extension,
            stop,
            Some(crate::scan_budget::SCAN_TIME_BUDGET),
        )
    }))
    .unwrap_or_else(|_| {
        Err(Error::new(
            "structural detailed file search failed on pathological input",
        ))
    })
}

/// [`search_files`] with its time budget (`None` evaluates every candidate).
pub(crate) fn search_files_with_budget(
    options: StructuralSearchFilesOptions,
    demand: StructuralDemand<'_>,
    admit: &(dyn Fn(&Path) -> bool + Sync),
    select_extension: &(dyn Fn(&Path) -> String + Sync),
    stop: &Stop<'_>,
    budget: Option<std::time::Duration>,
) -> EngineResult<StructuralSearchFilesDetailedResult> {
    let StructuralSearchFilesOptions {
        path,
        pattern,
        rule,
        include,
        exclude,
        exclude_dir,
        default_exclude_dir,
        ignore,
        max_depth,
        max_files,
        skip_files,
        max_file_bytes,
    } = options;
    let pattern_ref = pattern.as_deref();
    let rule_ref = rule.as_ref();
    let query = match StructuralQuery::new(pattern_ref, rule_ref) {
        Ok(query) => query,
        Err(message) => return Ok(invalid_query_result(path, pattern_ref, rule_ref, message)),
    };

    let root = PathBuf::from(&path);
    if !admit(&root) {
        return Err(Error::new(
            "Structural search root is denied by path policy",
        ));
    }
    check_root_exists(&root)?;
    let timed = max_files.is_none();
    let max_files = max_files.map_or(crate::scan_budget::walk_ceiling(), |n| n as usize);
    let max_file_bytes = max_file_bytes
        .map(|n| n as u64)
        .unwrap_or(crate::signatures::MAX_PARSE_SIZE as u64);
    let prefilter = query.prefilter();
    let query_explanation = query.explanation_with_prefilter(&prefilter);

    let globs = crate::search::glob::compile(
        &root,
        &include.unwrap_or_default(),
        &exclude.unwrap_or_default(),
    )?;
    let (mut candidate_files, walk_errors) = collect_files(
        &root,
        &Walked {
            globs: &globs,
            prune: Prune::new(exclude_dir.iter().flatten()),
            defaults: default_exclude_dir.map(|names| Prune::new(names).at(&root)),
            ignore,
            max_depth,
            max_files: max_files.saturating_add(1),
        },
        admit,
        stop,
    )?;
    let scan_truncated = candidate_files.len() > max_files;
    // The over-read candidate is the first the walk cap left out.
    let scan_frontier = candidate_files.get(max_files).cloned();
    candidate_files.truncate(max_files);
    // Walk order is stable, so the leading candidates an earlier window
    // evaluated are exactly these.
    let skip = skip_files
        .map_or(0, |n| n as usize)
        .min(candidate_files.len());
    candidate_files.drain(..skip);
    // Compile one matcher per extension up front (cheap, serial), then read,
    // prefilter, parse and match files in parallel. `collect` keeps candidate
    // order, and the serial fold below rebuilds the same counters.
    let scan = FileScan {
        admit,
        stop,
        select_extension,
        anchors: AnchorFilter::new(&prefilter).map_err(Error::new)?,
        matchers: compile_matchers(&candidate_files, select_extension, &query),
        max_file_bytes,
        demand,
    };
    let mut pacer = crate::scan_budget::ScanPacer::new(budget.filter(|_| timed));
    let mut outcomes = Vec::with_capacity(candidate_files.len());
    let mut scan_resume = None;
    let mut rest = candidate_files.as_slice();
    while !rest.is_empty() {
        if pacer.spent() {
            scan_resume = Some(u32::try_from(skip + outcomes.len()).unwrap_or(u32::MAX));
            break;
        }
        let (chunk, tail) = rest.split_at(pacer.next_chunk().min(rest.len()));
        rest = tail;
        outcomes.extend(
            chunk
                .par_iter()
                .map(|file_path| scan.file(file_path))
                .collect::<EngineResult<Vec<_>>>()?,
        );
        pacer.record(chunk.len());
    }
    let scan_truncated = scan_truncated || scan_resume.is_some();
    // The walk cap of an unset `max_files` resumes where the walk stopped.
    if timed && scan_resume.is_none() && scan_truncated {
        scan_resume = Some(u32::try_from(skip + candidate_files.len()).unwrap_or(u32::MAX));
    }

    let mut counts = ScanCounts {
        walk_errors: walk_errors.skipped,
        ..ScanCounts::default()
    };
    let mut files = Vec::with_capacity(outcomes.len());
    for (tally, file) in outcomes.into_iter().flatten() {
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
        scan_resume,
        scan_frontier,
        files,
        total_matches: counts.total_matches,
        parsed_files: counts.parsed_files,
        skipped_by_pre_filter: counts.skipped_by_pre_filter,
        skipped_unsupported: counts.skipped_unsupported,
        skipped_unreadable: counts.skipped_unreadable,
        skipped_large: counts.skipped_large,
        walk_errors,
        status: status.to_owned(),
        query: query_explanation,
        diagnostics,
        warnings,
    })
}

type CompiledMatchers = BTreeMap<String, Result<OctoCompiledMatcher, CompileError>>;

fn invalid_query_result(
    path: String,
    pattern: Option<&str>,
    rule: Option<&serde_json::Value>,
    message: String,
) -> StructuralSearchFilesDetailedResult {
    let diagnostic = invalid_query_diagnostic(message.clone()).with_path(path);
    StructuralSearchFilesDetailedResult {
        scan_truncated: false,
        scan_resume: None,
        scan_frontier: None,
        files: Vec::new(),
        total_matches: 0,
        parsed_files: 0,
        skipped_by_pre_filter: 0,
        skipped_unsupported: 0,
        skipped_unreadable: 0,
        skipped_large: 0,
        walk_errors: WalkErrors::default(),
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
