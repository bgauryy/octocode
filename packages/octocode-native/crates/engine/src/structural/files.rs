use std::collections::HashMap;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::overrides::Override;
use rayon::prelude::*;

use super::language::AgLanguage;
use super::octo::{ExecutionError, OctoCompiledMatcher, compile_matcher};
use super::query::{Prefilter, StructuralQuery, invalid_query_explanation};
use super::types::{
    StructuralDetailedMatch, StructuralDiagnostic, StructuralSearchDetailedFileResult,
    StructuralSearchFilesDetailedResult, StructuralSearchFilesOptions,
};
use crate::search::walk::{WalkFlags, build_overrides, walk_builder};
use crate::signatures::languages;
use crate::text::file_extension::extension_of;

/// Test helper: [`search_files_detailed_filtered_with_extension`] with no
/// path policy and each file's own extension.
#[cfg(test)]
pub(crate) fn search_files_detailed(
    options: StructuralSearchFilesOptions,
) -> Result<StructuralSearchFilesDetailedResult, String> {
    search_files_detailed_filtered_with_extension(options, &|_| Ok(true), &|path| {
        extension_for_path(path).unwrap_or_default()
    })
}

/// Detailed structural search with caller-owned descendant policy and
/// cancellation, plus a caller-selected parser for ambiguous paths. The
/// `allow_path` callback runs before candidate accounting, literal prefilter
/// reads, metadata reads, and source reads; file paths remain unchanged in
/// diagnostics and match IDs.
pub fn search_files_detailed_filtered_with_extension(
    options: StructuralSearchFilesOptions,
    allow_path: &(dyn Fn(&Path) -> Result<bool, String> + Sync),
    select_extension: &(dyn Fn(&Path) -> String + Sync),
) -> Result<StructuralSearchFilesDetailedResult, String> {
    let StructuralSearchFilesOptions {
        path,
        pattern,
        rule,
        include,
        exclude,
        exclude_dir,
        hidden,
        no_ignore,
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
    if !allow_path(&root)? {
        return Err("Structural search root is denied by path policy".to_owned());
    }
    check_root_exists(&root)?;
    let max_files = max_files.map(|n| n as usize).unwrap_or(2_000);
    let max_file_bytes = max_file_bytes
        .map(|n| n as u64)
        .unwrap_or(crate::signatures::MAX_PARSE_SIZE as u64);
    let prefilter = query.prefilter();
    let query_explanation = query.explanation_with_prefilter(&prefilter);

    let overrides = build_overrides(
        &root,
        &include.unwrap_or_default(),
        &exclude.unwrap_or_default(),
    )?;
    let mut candidate_files = collect_files(
        &root,
        overrides,
        &exclude_dir.unwrap_or_default(),
        max_files.saturating_add(1),
        hidden,
        no_ignore,
        max_depth,
        allow_path,
        &|_| true,
    )?;
    let scan_truncated = candidate_files.len() > max_files;
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
        allow_path,
        select_extension,
        anchors: AnchorFilter::new(&prefilter)?,
        matchers: compile_matchers(&candidate_files, select_extension, &query),
        max_file_bytes,
    };
    let outcomes = candidate_files
        .par_iter()
        .map(|file_path| scan.file(file_path))
        .collect::<Result<Vec<_>, String>>()?;

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

type CompiledMatchers = BTreeMap<String, Result<OctoCompiledMatcher, String>>;

fn invalid_query_result(
    path: String,
    pattern: Option<&str>,
    rule: Option<&str>,
    message: String,
) -> StructuralSearchFilesDetailedResult {
    let diagnostic = StructuralDiagnostic::new(
        "structural.query.invalid",
        "error",
        "match",
        message.clone(),
    )
    .with_path(path)
    .with_recovery("Provide exactly one non-empty structural pattern or YAML rule.");
    StructuralSearchFilesDetailedResult {
        scan_truncated: false,
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
            let lang = AgLanguage::new(&ext, entry);
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
    allow_path: &'a (dyn Fn(&Path) -> Result<bool, String> + Sync),
    select_extension: &'a (dyn Fn(&Path) -> String + Sync),
    anchors: Option<AnchorFilter>,
    matchers: CompiledMatchers,
    max_file_bytes: u64,
}

impl FileScan<'_> {
    /// Gate, read, and match one candidate. `None` when path policy denies it.
    fn file(&self, file_path: &Path) -> Result<FileOutcome, String> {
        if !(self.allow_path)(file_path)? {
            return Ok(None);
        }
        let path_string = file_path.to_string_lossy().to_string();
        // One read serves both the anchor check and the parse.
        let prefiltered = self
            .anchors
            .as_ref()
            .map(|anchors| anchors.check(file_path, self.max_file_bytes));
        if matches!(prefiltered, Some(Anchored::Miss)) {
            return Ok(Some((Tally::PreFilter, skipped_file(
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
            ))));
        }

        let ext = (self.select_extension)(file_path);
        let Some(entry) = languages::find_entry(&ext) else {
            return Ok(Some((
                Tally::Unsupported,
                skipped_file(
                    path_string.clone(),
                    "unsupported",
                    "unsupportedExtension",
                    StructuralDiagnostic::new(
                        "structural.language.unsupported",
                        "warning",
                        "parse",
                        format!("Structural search does not support .{ext} files."),
                    )
                    .with_path(path_string)
                    .with_recovery(
                        "Use text search for this extension or add a tree-sitter grammar mapping.",
                    ),
                ),
            )));
        };

        let content = match prefiltered {
            None | Some(Anchored::Miss) => read_source(file_path, self.max_file_bytes),
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
        Ok(Some(self.match_source(
            path_string,
            &ext,
            entry.language_id,
            &content,
        )))
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
            Err(message) => {
                if let Some(error) = ExecutionError::from_compile_message(message) {
                    let diagnostic = error.diagnostic(&path_string);
                    return (
                        Tally::None,
                        skipped_file(path_string, "truncated", "queryCompile", diagnostic),
                    );
                }
                return (Tally::CompileFailure, skipped_file(
                    path_string.clone(),
                    "parserFailed",
                    "queryCompile",
                    StructuralDiagnostic::new(
                        "structural.query.compileFailed",
                        "error",
                        "match",
                        message.clone(),
                    )
                    .with_path(path_string)
                    .with_recovery("Check the structural pattern or YAML rule against this file's language grammar."),
                ));
            }
        };

        let matches: Vec<StructuralDetailedMatch> = match run(content) {
            Ok(matches) => matches,
            Err(error) => {
                let diagnostic = error.diagnostic(&path_string);
                return (
                    Tally::None,
                    skipped_file(path_string, "truncated", "executionLimit", diagnostic),
                );
            }
        }
        .into_iter()
        .map(|m| StructuralDetailedMatch::from_match(m.matched, m.node_kind))
        .collect();
        (
            Tally::Parsed(matches.len() as u32),
            StructuralSearchDetailedFileResult {
                path: path_string,
                status: "ok".to_owned(),
                language_id: language_id.map(str::to_owned),
                skipped_reason: None,
                matches,
                diagnostics: Vec::new(),
            },
        )
    }
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
            )
            .with_recovery(
                "Raise maxFileBytes or inspect the file with a narrower text search first.",
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

/// Read a candidate's UTF-8 source behind the size gate: metadata first, so
/// an oversized file is never read.
fn read_source(path: &Path, max_file_bytes: u64) -> Result<String, SourceSkip> {
    let len = fs::metadata(path).map_err(SourceSkip::Metadata)?.len();
    if len > max_file_bytes {
        return Err(SourceSkip::Large(len));
    }
    fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::InvalidData {
            SourceSkip::NotUtf8(err)
        } else {
            SourceSkip::Unreadable(err)
        }
    })
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
    /// No anchor in the file, or the file could not be read.
    Miss,
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
        use std::io::Read as _;
        let Ok(mut file) = fs::File::open(path) else {
            return Anchored::Miss;
        };
        let Ok(len) = file.metadata().map(|meta| meta.len()) else {
            return Anchored::Miss;
        };
        if len > max_file_bytes {
            let hit = match &self.automaton {
                None => true,
                Some(automaton) => automaton
                    .try_stream_find_iter(&mut file)
                    .ok()
                    .and_then(|mut found| found.next())
                    .is_some_and(|found| found.is_ok()),
            };
            return if hit {
                Anchored::Large(len)
            } else {
                Anchored::Miss
            };
        }
        let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0).saturating_add(1));
        if file.read_to_end(&mut bytes).is_err() {
            return Anchored::Miss;
        }
        if self
            .automaton
            .as_ref()
            .is_some_and(|automaton| !automaton.is_match(&bytes))
        {
            return Anchored::Miss;
        }
        // A file that grew past the cap while it was read is skipped as large.
        if bytes.len() as u64 > max_file_bytes {
            return Anchored::Large(bytes.len() as u64);
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

/// Loud existence check shared by every entry point — mirrors the message
/// `collect_files` produces so all prefilter branches fail identically.
fn check_root_exists(root: &Path) -> Result<(), String> {
    fs::metadata(root).map(|_| ()).map_err(|err| {
        format!(
            "Cannot access structural search path '{}': {err}",
            root.display()
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_files(
    root: &Path,
    overrides: Override,
    exclude_dir: &[String],
    max_files: usize,
    hidden: Option<bool>,
    no_ignore: Option<bool>,
    max_depth: Option<u32>,
    allow_path: &(dyn Fn(&Path) -> Result<bool, String> + Sync),
    accept_file: &dyn Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>, String> {
    if !allow_path(root)? {
        return Err("Structural search root is denied by path policy".to_owned());
    }
    let metadata = fs::metadata(root).map_err(|err| {
        format!(
            "Cannot access structural search path '{}': {err}",
            root.display()
        )
    })?;

    if metadata.is_file() {
        return Ok(
            if !overrides.matched(root, false).is_ignore() && accept_file(root) {
                vec![root.to_path_buf()]
            } else {
                Vec::new()
            },
        );
    }
    if !metadata.is_dir() {
        return Ok(Vec::new());
    }

    let excluded: HashSet<String> = exclude_dir.iter().cloned().collect();
    // Unlike the text lane, `no_ignore` keeps the global gitignore,
    // `.git/info/exclude`, and parent ignore files, and `max_depth` counts the
    // root's entries as depth 1.
    let mut builder = walk_builder(
        root,
        &WalkFlags {
            hidden: hidden == Some(true),
            no_ignore: no_ignore == Some(true),
            no_ignore_global: false,
            max_depth: max_depth.map(|n| n as usize),
        },
    );
    builder
        .overrides(overrides)
        .sort_by_file_path(Ord::cmp)
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                return !excluded.contains(name.as_ref());
            }
            true
        });

    let mut out = Vec::new();
    for result in builder.build() {
        if out.len() >= max_files {
            break;
        }
        let Ok(entry) = result else { continue };
        if !allow_path(entry.path())? {
            continue;
        }
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.into_path();
        if accept_file(&path) {
            out.push(path);
        }
    }
    Ok(out)
}

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
        status: status.to_owned(),
        language_id: None,
        skipped_reason: Some(skipped_reason.to_owned()),
        matches: Vec::new(),
        diagnostics: vec![diagnostic],
    }
}

// ── Structural rewrite file walker ──────────────────────────────────────────

/// Result for a single file from a structural rewrite file-tree scan.
pub struct StructuralRewriteFileResult {
    pub path: String,
    pub matches: Vec<super::rewrite::StructuralRewriteMatch>,
    /// ERROR/MISSING nodes in the scanned source, from the same parse that
    /// produced `matches` (the pre-rewrite side of the syntax-regression check).
    pub syntax_errors: u32,
}

/// Aggregate result of a structural rewrite file-tree scan, including coverage
/// accounting. The counters let `astRewrite` report how much of the corpus the
/// rewrite actually covered.
#[derive(Default)]
pub struct StructuralRewriteFilesResult {
    /// Files that produced at least one rewrite match.
    pub files: Vec<StructuralRewriteFileResult>,
    /// Candidate scan hit `max_files`; files beyond it were not evaluated.
    pub scan_truncated: bool,
    /// Candidate files that could not be read (vanished, permission, IO).
    pub skipped_unreadable: u32,
    /// Candidate files larger than `max_file_bytes`.
    pub skipped_large: u32,
    /// Candidate files whose bytes are not valid UTF-8.
    pub skipped_binary: u32,
    /// Candidate files the rewrite engine rejected (parse/apply error, or the
    /// per-file match cap).
    pub skipped_errored: u32,
}

/// Walk a file tree and apply an ast-grep inline-rule rewrite to every
/// candidate file in parallel. Files that produce no matches are omitted from
/// `files`; files that error, exceed the byte limit, are non-UTF8, or cannot be
/// read are counted in the coverage fields instead of being dropped silently.
///
/// The rule config must be a complete ast-grep inline-rule object (language,
/// rule, fix, etc.), as accepted by ast-grep's `--inline-rules`.
pub fn rewrite_files(
    options: super::types::StructuralRewriteFilesOptions,
) -> Result<StructuralRewriteFilesResult, String> {
    let root = std::path::PathBuf::from(&options.path);
    check_root_exists(&root)?;

    let rule_config: serde_json::Value = serde_json::from_str(&options.rule_config_json)
        .map_err(|e| format!("[structural.rewrite.json] {e}"))?;
    let selector = rule_config
        .get("language")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "[structural.rewrite.invalid] language is required".to_owned())?
        .to_owned();
    let extensions = super::rewrite::rewrite_language_extensions(&selector)
        .ok_or_else(|| format!("[structural.rewrite.invalid] {selector} is not supported"))?;

    let include = options.include.unwrap_or_default();
    let exclude = options.exclude.unwrap_or_default();
    let exclude_dir = options.exclude_dir.unwrap_or_default();
    let max_files = options.max_files.map(|n| n as usize).unwrap_or(2_000);
    let max_file_bytes = options
        .max_file_bytes
        .map_or(crate::signatures::MAX_PARSE_SIZE as u64, u64::from);

    let overrides = build_overrides(&root, &include, &exclude)?;
    let is_single_file = root.is_file();
    let candidate_files = collect_files(
        &root,
        overrides,
        &exclude_dir,
        max_files.saturating_add(1),
        options.hidden,
        options.no_ignore,
        options.max_depth,
        &|_| Ok(true),
        &|path| {
            is_single_file
                || extension_for_path(path)
                    .is_some_and(|extension| extensions.contains(extension.as_str()))
        },
    )?;
    // `collect_files` was asked for `max_files + 1`; if it returned more than
    // `max_files` the candidate scan was truncated and files beyond the cap were
    // never evaluated.
    let scan_truncated = candidate_files.len() > max_files;
    let candidate_files: Vec<_> = candidate_files.into_iter().take(max_files).collect();

    // The parser language varies per file (e.g. `.tsx` under `typescript`, `.h`
    // under `cpp`), so compile the rule once per distinct parser language before
    // the parallel scan instead of once per file.
    let parser_for = |path: &std::path::Path| -> String {
        super::rewrite::rewrite_parser_for_path(&selector, &path.to_string_lossy())
    };
    let mut compiled: HashMap<String, Result<super::rewrite::CompiledRewrite, String>> =
        HashMap::new();
    for path in &candidate_files {
        compiled
            .entry(parser_for(path))
            .or_insert_with_key(|parser| {
                let mut file_rule = rule_config.clone();
                if let Some(config) = file_rule.as_object_mut() {
                    config.insert("language".to_owned(), serde_json::json!(parser));
                }
                super::rewrite::compile_rewrite(file_rule)
            });
    }

    // Per-thread skip accounting, mirroring the search path's coverage counters.
    use std::sync::atomic::{AtomicU32, Ordering};
    let skipped_unreadable = AtomicU32::new(0);
    let skipped_large = AtomicU32::new(0);
    let skipped_binary = AtomicU32::new(0);
    let skipped_errored = AtomicU32::new(0);

    let files: Vec<StructuralRewriteFileResult> = candidate_files
        .par_iter()
        .filter_map(|path| {
            let content = match read_source(path, max_file_bytes) {
                Ok(content) => content,
                Err(skip) => {
                    match skip {
                        SourceSkip::Metadata(_) | SourceSkip::Unreadable(_) => &skipped_unreadable,
                        SourceSkip::Large(_) => &skipped_large,
                        SourceSkip::NotUtf8(_) => &skipped_binary,
                    }
                    .fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            };
            let result = match compiled.get(&parser_for(path)) {
                Some(Ok(rewrite)) => {
                    rewrite.scan_with(&content, super::rewrite::CountErrors::WhenMatched)
                }
                Some(Err(error)) => Err(error.clone()),
                None => Err("[structural.rewrite.invalid] no compiled rule".to_owned()),
            };
            match result {
                Ok(scan) if scan.matches.is_empty() => None,
                Ok(scan) => Some(StructuralRewriteFileResult {
                    path: path.to_string_lossy().into_owned(),
                    matches: scan.matches,
                    syntax_errors: scan.syntax_errors,
                }),
                Err(_) => {
                    skipped_errored.fetch_add(1, Ordering::Relaxed);
                    None
                }
            }
        })
        .collect();

    Ok(StructuralRewriteFilesResult {
        files,
        scan_truncated,
        skipped_unreadable: skipped_unreadable.into_inner(),
        skipped_large: skipped_large.into_inner(),
        skipped_binary: skipped_binary.into_inner(),
        skipped_errored: skipped_errored.into_inner(),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod rewrite_coverage_tests {
    use super::*;

    #[test]
    fn cpp_rewrite_scans_h_headers_without_touching_c_sources() {
        let root = std::env::temp_dir().join(format!(
            "octocode-rewrite-cpp-header-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).expect("fixture root");
        fs::write(root.join("widget.h"), "void widget() { oldCall(); }\n").expect("C++ header");
        fs::write(root.join("legacy.c"), "void legacy() { oldCall(); }\n").expect("C source");
        let options = crate::structural::StructuralRewriteFilesOptions {
            path: root.to_string_lossy().into_owned(),
            rule_config_json: serde_json::json!({
                "id":"octocode-inline-rewrite",
                "language":"cpp",
                "rule":{"pattern":"oldCall()"},
                "fix":"newCall()"
            })
            .to_string(),
            include: None,
            exclude: None,
            exclude_dir: None,
            hidden: Some(false),
            no_ignore: Some(false),
            max_depth: None,
            max_files: Some(1),
            max_file_bytes: Some(1_000_000),
        };
        let result = rewrite_files(options).expect("rewrite files");
        let _ = fs::remove_dir_all(&root);
        assert!(
            !result.scan_truncated,
            "unrelated C files must not consume the scan cap"
        );
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].path.ends_with("widget.h"));
    }

    #[test]
    fn family_rewrite_uses_each_files_registered_parser() {
        let root = std::env::temp_dir().join(format!(
            "octocode-rewrite-tsx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).expect("fixture root");
        fs::write(root.join("widget.tsx"), "const widget = <Old />;\n").expect("TSX source");
        let options = crate::structural::StructuralRewriteFilesOptions {
            path: root.to_string_lossy().into_owned(),
            rule_config_json: serde_json::json!({
                "id":"octocode-inline-rewrite",
                "language":"typescript",
                "rule":{"pattern":"<Old />"},
                "fix":"<New />"
            })
            .to_string(),
            include: None,
            exclude: None,
            exclude_dir: None,
            hidden: Some(false),
            no_ignore: Some(false),
            max_depth: None,
            max_files: Some(10),
            max_file_bytes: Some(1_000_000),
        };
        let result = rewrite_files(options).expect("rewrite files");
        let _ = fs::remove_dir_all(&root);
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].path.ends_with("widget.tsx"));
    }

    // Regression: a rewrite over a dir containing a non-UTF8 candidate file must
    // account for the skip in the coverage counters, not drop it silently.
    #[test]
    fn rewrite_files_reports_skipped_non_utf8_and_oversized_files() {
        let root = std::env::temp_dir().join(format!(
            "octocode-rewrite-skip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("fixture root");
        // A matchable, valid file.
        fs::write(root.join("good.rs"), "fn main() { oldCall(); }\n").expect("good file");
        // A non-UTF8 file with a supported extension.
        fs::write(root.join("bad.rs"), [0xff_u8, 0xfe, 0xfd, 0x00]).expect("bad file");
        // An oversized file (exceeds the tiny max_file_bytes below).
        fs::write(root.join("big.rs"), "fn main() { oldCall(); }\n".repeat(64)).expect("big file");

        let options = crate::structural::StructuralRewriteFilesOptions {
            path: root.to_string_lossy().into_owned(),
            rule_config_json: serde_json::json!({
                "id":"octocode-inline-rewrite",
                "language":"rust",
                "rule":{"pattern":"oldCall()"},
                "fix":"newCall()"
            })
            .to_string(),
            include: None,
            exclude: None,
            exclude_dir: None,
            hidden: Some(false),
            no_ignore: Some(false),
            max_depth: None,
            max_files: Some(2_000),
            max_file_bytes: Some(64),
        };

        let result = rewrite_files(options).expect("rewrite files");
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            result.skipped_binary, 1,
            "the non-UTF8 file must be reported as a skip, not silently dropped"
        );
        assert_eq!(
            result.skipped_large, 1,
            "the oversized file must be reported as a skip, not silently dropped"
        );
        assert_eq!(
            result.files.len(),
            1,
            "only the small valid file should produce matches"
        );
    }
}
