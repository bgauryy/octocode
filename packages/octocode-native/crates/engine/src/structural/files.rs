use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::language::AgLanguage;
use super::octo::{CompileError, Found, OctoCompiledMatcher, compile_matcher};
use super::query::{
    Prefilter, StructuralQuery, invalid_query_diagnostic, invalid_query_explanation,
};
use super::types::{
    FileMatches, StructuralDemand, StructuralDetailedMatch, StructuralDiagnostic,
    StructuralSearchDetailedFileResult, StructuralSearchFilesDetailedResult,
    StructuralSearchFilesOptions,
};
use crate::error::{Error, Result as EngineResult};
use crate::search::glob::Globs;
use crate::search::walk::{IgnorePolicy, Prune, RootedPrune, Stop, walk_builder};
use crate::signatures::languages;
use crate::text::file_extension::extension_of;

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
    let rule_ref = rule.as_deref();
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
    let mut candidate_files = collect_files(
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

    let mut counts = ScanCounts::default();
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
    rule: Option<&str>,
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

/// How one candidate file counts toward the scan totals.
enum Tally {
    None,
    PreFilter,
    Unsupported,
    Unreadable,
    Large,
    CompileFailure,
    Parsed(u32),
}

type FileOutcome = Option<(Tally, StructuralSearchDetailedFileResult)>;

/// Shared, read-only state of the parallel per-file scan.
struct FileScan<'a> {
    admit: &'a (dyn Fn(&Path) -> bool + Sync),
    stop: &'a Stop<'a>,
    select_extension: &'a (dyn Fn(&Path) -> String + Sync),
    anchors: Option<AnchorFilter>,
    matchers: CompiledMatchers,
    max_file_bytes: u64,
    demand: StructuralDemand<'a>,
}

impl FileScan<'_> {
    /// Gate, read, and match one candidate. `None` when path policy denies it.
    fn file(&self, file_path: &Path) -> EngineResult<FileOutcome> {
        if (self.stop)() {
            return Err(Error::cancelled());
        }
        if !(self.admit)(file_path) {
            return Ok(None);
        }
        let path_string = file_path.to_string_lossy().to_string();
        // One read serves both the anchor check and the parse.
        let prefiltered = self
            .anchors
            .as_ref()
            .map(|anchors| anchors.check(file_path, self.max_file_bytes));
        if let Some(Anchored::Miss(source_sha256)) = &prefiltered {
            let mut file = skipped_file(
                path_string,
                "skippedByPreFilter",
                "preFilter",
                StructuralDiagnostic::new(
                    "structural.prefilter.skipped",
                    "info",
                    "scan",
                    "File excluded by the text pre-filter (it does not contain the pattern's anchor literal), so AST parsing was skipped.",
                )
                .with_recovery("Remove the literal prefilter by using a rule with no safe anchor if every file must be parsed."),
            );
            file.source_sha256 = source_sha256.clone();
            return Ok(Some((Tally::PreFilter, file)));
        }

        let ext = (self.select_extension)(file_path);
        let Some(entry) = languages::find_entry(&ext) else {
            return Ok(Some((
                Tally::Unsupported,
                skipped_file(
                    path_string.clone(),
                    "unsupported",
                    "unsupportedExtension",
                    unsupported_language(&path_string),
                ),
            )));
        };

        let content = match prefiltered {
            None | Some(Anchored::Miss(_)) => read_source(file_path, self.max_file_bytes),
            Some(Anchored::Hit(bytes)) => source_text(bytes),
            Some(Anchored::Large(len)) => Err(SourceSkip::Large(len)),
        };
        let content = match content {
            Ok(content) => content,
            Err(skip) => {
                return Ok(Some(source_skipped(
                    path_string,
                    &skip,
                    self.max_file_bytes,
                )));
            }
        };
        let (tally, mut file) = self.match_source(path_string, &ext, entry.language_id, &content);
        file.source_sha256 = Some(crate::digest::sha256(content.as_bytes()));
        Ok(Some((tally, file)))
    }

    fn match_source(
        &self,
        path_string: String,
        ext: &str,
        language_id: Option<&str>,
        content: &str,
    ) -> (Tally, StructuralSearchDetailedFileResult) {
        let Some(compiled) = self.matchers.get(ext) else {
            return (
                Tally::CompileFailure,
                skipped_file(
                    path_string.clone(),
                    "parserFailed",
                    "queryCompile",
                    StructuralDiagnostic::new(
                        "structural.matcher.missing",
                        "error",
                        "match",
                        "Structural matcher was unavailable after compilation.",
                    )
                    .with_path(path_string)
                    .with_recovery(
                        "Retry the search; this indicates an internal matcher lifecycle issue.",
                    ),
                ),
            );
        };
        let run = match compiled {
            Ok(run) => run,
            Err(error) => {
                if let Some(interruption) = error.interruption() {
                    let diagnostic = interruption.diagnostic(&path_string);
                    return (
                        Tally::None,
                        skipped_file(path_string, "truncated", "queryCompile", diagnostic),
                    );
                }
                return (
                    Tally::CompileFailure,
                    skipped_file(
                        path_string.clone(),
                        "parserFailed",
                        "queryCompile",
                        error.diagnostic().with_path(path_string),
                    ),
                );
            }
        };

        let mut found = Found::new(self.demand);
        if let Err(error) = run(content, &mut found) {
            let diagnostic = error.diagnostic(&path_string);
            return (
                Tally::None,
                skipped_file(path_string, "truncated", "executionLimit", diagnostic),
            );
        }
        let recovered = found.recovered();
        let matches = found.into_file_matches();
        (
            Tally::Parsed(matches.len() as u32),
            StructuralSearchDetailedFileResult {
                path: path_string,
                source_sha256: None,
                status: "ok".to_owned(),
                language_id: language_id.map(str::to_owned),
                skipped_reason: None,
                matches,
                recovered,
                diagnostics: Vec::new(),
            },
        )
    }
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

/// The skipped-file row and tally for a source that failed its read gates.
fn source_skipped(
    path_string: String,
    skip: &SourceSkip,
    max_file_bytes: u64,
) -> (Tally, StructuralSearchDetailedFileResult) {
    let (tally, status, reason, diagnostic) = match skip {
        SourceSkip::Metadata(err) => (
            Tally::Unreadable,
            "unreadable",
            "metadata",
            StructuralDiagnostic::new(
                "structural.file.unreadable",
                "warning",
                "scan",
                format!("Could not read file metadata: {err}."),
            )
            .with_recovery("Retry if the file still exists and permissions allow reading it."),
        ),
        SourceSkip::Large(len) => (
            Tally::Large,
            "truncated",
            "maxFileBytes",
            StructuralDiagnostic::new(
                "structural.file.tooLarge",
                "warning",
                "scan",
                format!(
                    "File is {len} bytes, above the structural search limit of {max_file_bytes} bytes."
                ),
            ),
        ),
        SourceSkip::Unreadable(err) | SourceSkip::NotUtf8(err) => (
            Tally::Unreadable,
            "unreadable",
            "read",
            StructuralDiagnostic::new(
                "structural.file.unreadable",
                "warning",
                "scan",
                format!("Could not read file content as UTF-8: {err}."),
            )
            .with_recovery("Use binary inspection or text search for non-UTF-8 content."),
        ),
    };
    let diagnostic = diagnostic.with_path(path_string.clone());
    (tally, skipped_file(path_string, status, reason, diagnostic))
}

/// Why a candidate's source was not read.
enum SourceSkip {
    Metadata(std::io::Error),
    Large(u64),
    Unreadable(std::io::Error),
    NotUtf8(std::io::Error),
}

/// Read a candidate's UTF-8 source through the one bounded reader: an
/// oversized file is refused from its handle's size, never read whole.
fn read_source(path: &Path, max_file_bytes: u64) -> Result<String, SourceSkip> {
    match crate::text::read_source(path, max_file_bytes) {
        Ok(source) => source_text(source.bytes),
        Err(crate::text::ReadError::TooLarge(len)) => Err(SourceSkip::Large(len)),
        Err(crate::text::ReadError::Io(err)) => Err(SourceSkip::Unreadable(err)),
        Err(crate::text::ReadError::NotRegular) => Err(SourceSkip::Metadata(
            std::io::Error::other("not a regular file"),
        )),
    }
}

#[derive(Default)]
struct ScanCounts {
    total_matches: u32,
    parsed_files: u32,
    skipped_by_pre_filter: u32,
    skipped_unsupported: u32,
    skipped_unreadable: u32,
    skipped_large: u32,
    compile_failures: u32,
}

impl ScanCounts {
    fn record(&mut self, tally: Tally) {
        match tally {
            Tally::None => {}
            Tally::PreFilter => self.skipped_by_pre_filter += 1,
            Tally::Unsupported => self.skipped_unsupported += 1,
            Tally::Unreadable => self.skipped_unreadable += 1,
            Tally::Large => self.skipped_large += 1,
            Tally::CompileFailure => self.compile_failures += 1,
            Tally::Parsed(count) => {
                self.parsed_files += 1;
                self.total_matches = self.total_matches.saturating_add(count);
            }
        }
    }

    fn warnings(&self, prefilter: &Prefilter, is_rule: bool, max_file_bytes: u64) -> Vec<String> {
        let Self {
            parsed_files,
            skipped_by_pre_filter,
            skipped_unsupported,
            skipped_unreadable,
            skipped_large,
            ..
        } = self;
        let mut warnings = Vec::new();
        match prefilter {
            Prefilter::None => {
                warnings.push(format!(
                    "No literal anchor in the {} — parsed all {parsed_files} supported candidate file(s) with no text pre-filter.",
                    if is_rule { "rule" } else { "pattern" }
                ));
            }
            Prefilter::Single(_) if *skipped_by_pre_filter > 0 => {
                warnings.push(format!(
                    "Pre-filter skipped parsing {skipped_by_pre_filter} file(s); parsed {parsed_files}."
                ));
            }
            Prefilter::Union(anchors) if *skipped_by_pre_filter > 0 => {
                warnings.push(format!(
                    "Union pre-filter ({} anchors: {}) skipped {skipped_by_pre_filter} file(s); parsed {parsed_files}.",
                    anchors.len(),
                    anchors.join("|")
                ));
            }
            _ => {}
        }
        if *skipped_unsupported > 0 {
            warnings.push(format!(
                "Skipped {skipped_unsupported} candidate file(s) with unsupported extensions."
            ));
        }
        if *skipped_unreadable > 0 {
            warnings.push(format!(
                "Skipped {skipped_unreadable} unreadable or vanished candidate file(s)."
            ));
        }
        if *skipped_large > 0 {
            warnings.push(format!(
                "Skipped {skipped_large} candidate file(s) larger than {max_file_bytes} bytes."
            ));
        }
        warnings
    }

    fn status(&self, files: &[StructuralSearchDetailedFileResult]) -> &'static str {
        if files.iter().any(|file| file.status == "truncated") {
            "truncated"
        } else if self.compile_failures > 0 {
            "parserFailed"
        } else if self.parsed_files == 0
            && self.skipped_unsupported > 0
            && self.skipped_by_pre_filter == 0
            && self.skipped_unreadable == 0
            && self.skipped_large == 0
        {
            "unsupported"
        } else if self.skipped_unsupported > 0
            || self.skipped_unreadable > 0
            || self.skipped_large > 0
        {
            "partial"
        } else {
            "ok"
        }
    }
}

/// The literal prefilter: a file that holds none of the pattern's anchors
/// cannot match, so it is skipped unparsed.
struct AnchorFilter {
    /// `None` when an anchor is empty: every readable file holds it.
    automaton: Option<aho_corasick::AhoCorasick>,
}

/// What the prefilter learned about one candidate.
enum Anchored {
    /// No anchor, with a digest when the source was read whole.
    Miss(Option<String>),
    /// An anchor is present; the bytes read for the check, kept for the parse.
    Hit(Vec<u8>),
    /// An anchor is present in a file over the parse cap (its length).
    Large(u64),
}

impl AnchorFilter {
    fn new(prefilter: &Prefilter) -> Result<Option<Self>, String> {
        let anchors = match prefilter {
            Prefilter::None => return Ok(None),
            Prefilter::Single(anchor) => std::slice::from_ref(anchor),
            Prefilter::Union(anchors) => anchors.as_slice(),
        };
        // An empty anchor matches every file; short-circuit those instead of
        // feeding an empty pattern to Aho-Corasick. Otherwise build one
        // automaton for all anchors and scan each file in a single linear pass.
        let automaton = if anchors.iter().any(String::is_empty) {
            None
        } else {
            Some(aho_corasick::AhoCorasick::new(anchors).map_err(|error| error.to_string())?)
        };
        Ok(Some(Self { automaton }))
    }

    /// Read `path` once. A file within `max_file_bytes` is read whole and its
    /// bytes kept for the parse; a larger one is streamed only until the first
    /// anchor, never held in memory, since the parse skips it anyway.
    fn check(&self, path: &Path, max_file_bytes: u64) -> Anchored {
        let bytes = match crate::text::read_source(path, max_file_bytes) {
            Ok(source) => source.bytes,
            Err(crate::text::ReadError::TooLarge(len)) => {
                let hit = match &self.automaton {
                    None => true,
                    Some(automaton) => crate::text::source::open_regular(path)
                        .ok()
                        .and_then(|(mut file, _)| {
                            automaton.try_stream_find_iter(&mut file).ok()?.next()
                        })
                        .is_some_and(|found| found.is_ok()),
                };
                return if hit {
                    Anchored::Large(len)
                } else {
                    Anchored::Miss(None)
                };
            }
            Err(_) => return Anchored::Miss(None),
        };
        if self
            .automaton
            .as_ref()
            .is_some_and(|automaton| !automaton.is_match(&bytes))
        {
            return Anchored::Miss(Some(crate::digest::sha256(&bytes)));
        }
        Anchored::Hit(bytes)
    }
}

/// The UTF-8 text of bytes the prefilter read, failing as
/// `fs::read_to_string` does on invalid UTF-8.
fn source_text(bytes: Vec<u8>) -> Result<String, SourceSkip> {
    String::from_utf8(bytes).map_err(|_| {
        SourceSkip::NotUtf8(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        ))
    })
}

/// Loud existence check shared by every entry point.
fn check_root_exists(root: &Path) -> EngineResult<std::fs::Metadata> {
    fs::metadata(root).map_err(|err| {
        Error::io(
            &err,
            format!(
                "Cannot access structural search path '{}': {err}",
                root.display()
            ),
        )
    })
}

/// What a structural walk takes: its globs, pruned directory names, ignore
/// policy, depth (the root's entries are depth 1), and how many files it
/// collects.
struct Walked<'a> {
    globs: &'a Globs,
    prune: Prune,
    /// The default excludes, pruned only where git ignores them.
    defaults: Option<RootedPrune>,
    ignore: IgnorePolicy,
    max_depth: Option<u32>,
    max_files: usize,
}

/// The candidate files under `root`, in path order. A file root is its own
/// candidate when the globs admit it.
fn collect_files(
    root: &Path,
    walked: &Walked<'_>,
    admit: &(dyn Fn(&Path) -> bool + Sync),
    stop: &Stop<'_>,
) -> EngineResult<Vec<PathBuf>> {
    let metadata = check_root_exists(root)?;
    if metadata.is_file() {
        let admitted = !walked.globs.overrides().matched(root, false).is_ignore();
        return Ok(if admitted {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        });
    }
    if !metadata.is_dir() {
        return Ok(Vec::new());
    }
    let mut builder = walk_builder(root, walked.ignore, walked.max_depth.map(|n| n as usize));
    let (prune, defaults) = (walked.prune.clone(), walked.defaults.clone());
    builder.filter_entry(move |entry| {
        !prune.prunes_entry(entry)
            && !(entry.depth() > 0
                && defaults.as_ref().is_some_and(|defaults| {
                    defaults.prunes(entry.path(), entry.file_type().is_some_and(|kind| kind.is_dir()))
                }))
    });
    builder
        .overrides(walked.globs.overrides())
        .sort_by_file_path(Ord::cmp);
    let mut out = Vec::new();
    for result in builder.build() {
        if out.len() >= walked.max_files {
            break;
        }
        if stop() {
            return Err(Error::cancelled());
        }
        let Ok(entry) = result else { continue };
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            // Let callers retain discovery metadata, including empty directories,
            // without counting them as source-file candidates.
            let _ = admit(entry.path());
            continue;
        }
        if entry.file_type().is_some_and(|ft| ft.is_symlink()) {
            // The walk never follows a link; the caller sees it to disclose
            // what it names, never as a candidate.
            let _ = admit(entry.path());
            continue;
        }
        if !entry.file_type().is_some_and(|ft| ft.is_file()) || !admit(entry.path()) {
            continue;
        }
        out.push(entry.into_path());
    }
    Ok(out)
}

#[cfg(test)]
fn extension_for_path(path: &Path) -> Option<String> {
    let extension = extension_of(&path.to_string_lossy(), true, "");
    (!extension.is_empty()).then_some(extension)
}

fn skipped_file(
    path: String,
    status: &str,
    skipped_reason: &str,
    diagnostic: StructuralDiagnostic,
) -> StructuralSearchDetailedFileResult {
    StructuralSearchDetailedFileResult {
        path,
        source_sha256: None,
        status: status.to_owned(),
        language_id: None,
        skipped_reason: Some(skipped_reason.to_owned()),
        matches: FileMatches::Held(Vec::new()),
        recovered: false,
        diagnostics: vec![diagnostic],
    }
}

/// One file's matches rebuilt for the page that shows them ([`file_matches`]).
pub enum Rebuilt {
    /// The scan's own matches, in its order.
    Matches(Vec<StructuralDetailedMatch>),
    /// The file no longer hashes to the scan's digest or no longer reads as
    /// the scan read it.
    Changed,
    /// The scan's bytes did not finish matching this time (an execution
    /// limit, such as the match deadline): its diagnostics say why.
    Incomplete(Vec<StructuralDiagnostic>),
}

impl Rebuilt {
    /// What matching the scan's own bytes gave: its matches, or the limit
    /// that cut it (the same bytes and query only differ in time spent).
    pub(super) fn of(result: super::StructuralSearchDetailedResult) -> Self {
        if result.status == "ok" {
            Self::Matches(result.matches)
        } else {
            Self::Incomplete(result.diagnostics)
        }
    }
}

/// The matches of one file a [`search_files`] scan evaluated, rebuilt for
/// the page that shows them (a [`StructuralDemand::Priced`] scan kept only
/// their prices): the file is read through the scan's bounded reader and
/// matched with the same query under `extension`'s grammar, so the matches
/// are the scan's own, in its order.
pub fn file_matches(
    path: &Path,
    extension: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
    source_sha256: &str,
    max_file_bytes: u64,
) -> EngineResult<Rebuilt> {
    std::panic::catch_unwind(|| {
        let Ok(content) = read_source(path, max_file_bytes) else {
            return Rebuilt::Changed;
        };
        if crate::digest::sha256(content.as_bytes()) != source_sha256 {
            return Rebuilt::Changed;
        }
        Rebuilt::of(super::evaluate_source(
            &content,
            &path.to_string_lossy(),
            extension,
            pattern,
            rule,
        ))
    })
    .map_err(|_| Error::new("structural file match failed on pathological input"))
}
