use crate::contracts::query_limits::ast_search::r#match as limits;
use crate::id::ToolId;
use crate::num::{u32_of, u32_of_usize};
use crate::response::result::ToolError;
use crate::tools::ast_rule::{
    GlobParsers, GrammarChoice, RuleError, choose_grammar, compile_check, count_grammars,
    has_extension_in, is_compile_failure, language_extensions, present_grammars,
};
use crate::tools::empty_walk::{FileProbe, Globs, Walk, diagnose};
use crate::{
    execution::cancel::CancellationCheck,
    policy::{path::PathPolicy, prune},
};
pub use octocode_contract_types::{AstSearchQueryMatchPattern, AstSearchQueryMatchRule};
use octocode_engine::structural::{
    FileMatches, StructuralDemand, StructuralDetailedMatch, StructuralDiagnostic,
    StructuralSearchFilesOptions,
};
use serde_json::{Value, json};

/// A `match` query in either of its generated forms (pattern or rule).
#[derive(Clone, Copy, Debug)]
pub enum MatchQuery<'a> {
    Pattern(&'a AstSearchQueryMatchPattern),
    Rule(&'a AstSearchQueryMatchRule),
}

/// Binds `$field` from whichever form `$query` is.
macro_rules! either_form {
    ($query:expr, $field:ident => $value:expr) => {
        crate::tools::variant_field::variant_field!($query, newtype [
            MatchQuery::Pattern => AstSearchQueryMatchPattern,
            MatchQuery::Rule => AstSearchQueryMatchRule,
        ] $field => $value)
    };
}

/// The wire rule as the JSON object the engine decodes. The typed rule holds
/// only serializable data, so it always converts.
fn to_rule_value(rule: &octocode_contract_types::AstSearchRule) -> Value {
    serde_json::to_value(rule).unwrap_or(Value::Null)
}

fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// Form-independent views in the engine's units.
impl<'a> MatchQuery<'a> {
    pub fn path(self) -> String {
        either_form!(self, path => path.to_string())
    }
    pub fn scope(self) -> Option<&'a octocode_contract_types::AstMatchScope> {
        either_form!(self, scope => scope.as_ref())
    }
    pub fn pattern(self) -> Option<String> {
        match self {
            Self::Pattern(query) => Some(query.pattern.to_string()),
            Self::Rule(_) => None,
        }
    }
    /// The typed rule as the engine's JSON rule object.
    pub fn rule(self) -> Option<Value> {
        match self {
            Self::Pattern(_) => None,
            Self::Rule(query) => Some(to_rule_value(&query.rule)),
        }
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include).map(|globs| crate::policy::include::include_globs(&globs)))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn default_excludes(self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        either_form!(self, default_excludes => default_excludes.defaults())
    }
    pub fn include_hidden(self) -> Option<bool> {
        either_form!(self, include_hidden => *include_hidden)
    }
    pub fn include_ignored(self) -> Option<bool> {
        either_form!(self, include_ignored => *include_ignored)
    }
    pub fn reverse(self) -> Option<bool> {
        either_form!(self, reverse => *reverse)
    }
    pub fn capture_text(self) -> Option<bool> {
        either_form!(self, capture_text => *capture_text)
    }
    /// Levels below `path` (1 = its children), as the wire counts them.
    pub fn max_depth(self) -> Option<u32> {
        either_form!(self, max_depth => max_depth.map(|depth| u32_of(depth.get())))
    }
    /// The caller's file scan cap; unset scans the whole scope within the
    /// scan time budget.
    pub fn max_files(self) -> Option<u32> {
        either_form!(self, max_files => max_files.map(|files| u32_of(files.get())))
    }
    /// Leading candidates (walk order) an earlier window evaluated.
    pub fn scan_offset(self) -> u32 {
        either_form!(self, scan_offset => scan_offset.map_or(0, |offset| u32_of(offset.get())))
    }
    /// The caller's matches per file page; unset fills the response window.
    pub fn match_page_size(self) -> Option<u32> {
        either_form!(self, match_page_size => match_page_size.map(|size| u32_of(size.get())))
    }
    pub fn match_content_length(self) -> Option<u32> {
        either_form!(self, match_content_length => Some(u32_of(*match_content_length)))
    }
    pub fn language(self) -> Option<String> {
        either_form!(self, language => language.as_ref().map(ToString::to_string))
    }
    pub fn language_globs(self) -> Option<&'a std::collections::BTreeMap<String, Vec<String>>> {
        either_form!(self, language_globs => (!language_globs.is_empty()).then_some(language_globs))
    }
    pub fn sort(self) -> Option<String> {
        either_form!(self, sort => Some(sort.to_string()))
    }
    pub fn result_view(self) -> Option<String> {
        either_form!(self, result_view => Some(result_view.to_string()))
    }
    pub fn page(self) -> u32 {
        either_form!(self, page => u32_of(*page))
    }
    pub fn match_page(self) -> u32 {
        either_form!(self, match_page => u32_of(*match_page))
    }
    /// The caller's files per page; unset fills the response window.
    pub fn page_size(self) -> Option<u32> {
        either_form!(self, page_size => page_size.map(|size| u32_of(size.get())))
    }
    pub fn snapshot(self) -> Option<String> {
        either_form!(self, snapshot => snapshot.as_ref().map(ToString::to_string))
    }
    /// The query as a continuation copies it; an object rule keeps only the
    /// matchers the caller set.
    fn to_value(self) -> Value {
        let mut value = match self {
            Self::Pattern(query) => serde_json::to_value(query),
            Self::Rule(query) => serde_json::to_value(query),
        }
        .unwrap_or_else(|_| json!({}));
        if let Some(rule) = value.get_mut("rule") {
            crate::response::result::remove_nulls(rule);
        }
        value
    }
}

/// The validated scope of one `match` query and the grammar it runs with.
struct Scope {
    canonical: std::path::PathBuf,
    is_file: bool,
    /// An explicit parser selector; otherwise each file selects its grammar.
    language: Option<String>,
    /// The grammar observed when the discovery window holds only one.
    inferred: Option<String>,
    extensions: Option<std::collections::BTreeSet<String>>,
    /// `languageGlobs`: the parser of files whose names select none.
    parsers: GlobParsers,
}

/// One scanned file.
struct ScannedFile {
    /// The row path.
    path: String,
    /// The parser extension the file was read with (empty for a single file,
    /// which no grammar census covers).
    parser: String,
    /// What the scan kept of its matches (see [`demand`]).
    matches: FileMatches,
    diagnostics: Vec<StructuralDiagnostic>,
    /// The engine status.
    status: String,
    /// The file parsed with recovered syntax that could hide a match.
    recovered: bool,
    /// The scanned path and the digest of the bytes it was matched in:
    /// where a page rebuilds a priced file's matches.
    source: Option<(std::path::PathBuf, String)>,
}

/// The files a scan evaluated plus its corpus-coverage signals. A
/// single-file scan leaves the signals at their complete defaults.
#[derive(Default)]
struct Scan {
    files: Vec<ScannedFile>,
    truncated: bool,
    /// Where a time-budget cut resumes (`scanOffset`), in walk order.
    scan_resume: Option<u32>,
    diagnostics: Vec<StructuralDiagnostic>,
    /// Unsupported, unreadable and over-large files skipped.
    skips: (u32, u32, u32),
    /// Entries the walk could not read (an unreadable directory): absence
    /// below them is unproven.
    walk_errors: octocode_engine::structural::WalkErrors,
    skipped_by_prefilter: u32,
    /// Every file the scan evaluated (absolute), for the memo's stamps.
    sources: Vec<std::path::PathBuf>,
    /// Canonical digest of complete source bytes already read by this scan.
    source_digest: String,
    /// Directory and ignore-file metadata observed during discovery, not source reads.
    discovery_sources: Vec<std::path::PathBuf>,
    /// Files outside the walk whose contents decide its ignore rules, and
    /// the repository markers whose presence does (empty when ignore rules
    /// are off): the memo watches them like the walked entries.
    ignore_rules: Vec<std::path::PathBuf>,
    repository_markers: Vec<std::path::PathBuf>,
    /// An ignore dependency no stamp can watch (a link, an unreadable
    /// entry) disables reuse.
    untracked_ignore_dependencies: bool,
    /// Candidate entries the security path policy withheld from the walk.
    withheld: crate::policy::discovery::Withheld,
    /// Files the walk passed over because no grammar parses their names.
    unparsed: NamedFiles,
    /// Without `language`: the files the walk admitted, by parser extension
    /// (the census the scope's grammar is inferred from).
    census: std::collections::BTreeMap<String, usize>,
    /// The scopes a scan cut at the walk ceiling never read.
    unwalked: Option<super::unwalked::Unwalked>,
}

/// Example paths a file-count warning names.
const NAMED_EXAMPLES: usize = 3;

/// A tally of files a warning names: their count and the first
/// [`NAMED_EXAMPLES`] paths in path order, spelled as the response's row
/// paths are (workspace-relative, else absolute).
#[derive(Default)]
struct NamedFiles {
    count: u32,
    examples: std::collections::BTreeSet<String>,
}

impl NamedFiles {
    fn record(&mut self, path: String) {
        self.count = self.count.saturating_add(1);
        self.examples.insert(path);
        if self.examples.len() > NAMED_EXAMPLES {
            self.examples.pop_last();
        }
    }

    /// The same tally with each path respelled by `spell`.
    fn respelled(&self, spell: impl Fn(&str) -> String) -> Self {
        Self {
            count: self.count,
            examples: self.examples.iter().map(|path| spell(path)).collect(),
        }
    }

    /// The paths a warning lists: `a, b, c, …` when more files follow.
    fn listed(&self) -> String {
        let more = if self.count as usize > self.examples.len() {
            ", …"
        } else {
            ""
        };
        format!(
            "{}{more}",
            self.examples.iter().cloned().collect::<Vec<_>>().join(", ")
        )
    }
}

/// The warning that names the files passed over for want of a grammar and
/// the field that parses them.
fn unparsed_notice(files: &NamedFiles) -> Option<String> {
    let names = files.listed();
    match files.count {
        0 => None,
        1 => Some(format!(
            "1 file skipped: no AST grammar for its name ({names}); if it is source code, languageGlobs maps its name to a grammar."
        )),
        count => Some(format!(
            "{count} files skipped: no AST grammar for their names ({names}); if they are source code, languageGlobs maps their names to a grammar."
        )),
    }
}

/// The warning that names the files whose recovered parse could hide a
/// match (see `octocode_engine::structural`'s recovery criterion): the
/// matches listed are not all of the file's.
fn recovered_notice(files: &NamedFiles) -> Option<String> {
    let names = files.listed();
    let (count, noun) = match files.count {
        0 => return None,
        1 => (1, "file"),
        count => (count, "files"),
    };
    Some(format!(
        "{count} {noun} parsed with syntax errors that could hide a match ({names}); astSearch syntaxTree shows where."
    ))
}

/// Scans kept for the continuation pages of a multi-page result.
static SCANS: super::memo::ScanMemo<Scan> = super::memo::ScanMemo::new();

/// One scan per distinct query at a time: identical rows of one batch wait
/// for the first one's scan and its start time (the memo's settle check).
static SCAN_FLIGHT: octocode_io::flight::KeyedFlight<
    String,
    Option<(std::sync::Arc<Scan>, std::time::SystemTime)>,
> = octocode_io::flight::KeyedFlight::transient();

/// The flight a row joins: every query field but its page cursors (rows that
/// differ only in the page they show scan alike), plus the scan inputs the
/// query does not name.
fn flight_key(q: MatchQuery<'_>, scan_key: &str) -> String {
    let mut row = q.to_value();
    if let Some(fields) = row.as_object_mut() {
        for field in ["page", "matchPage", "snapshot"] {
            fields.shift_remove(field);
        }
    }
    crate::digest::json_sha256(&json!([row, scan_key]))
}

/// The scan inputs a `match` snapshot does not digest; a stored scan is
/// served only to a page with the same ones. The row shape is one of them:
/// a priced scan's prices are the sizes of rows of that shape.
fn scan_key(q: MatchQuery<'_>, paths: &PathPolicy) -> String {
    crate::digest::json_sha256(&json!([
        paths.identity(),
        q.default_excludes(),
        super::MAX_PARSE_SOURCE_BYTES,
        q.capture_text().unwrap_or(false),
        content_length(q)
    ]))
}

/// The views that list files, not matches.
fn file_list(q: MatchQuery<'_>) -> bool {
    matches!(q.result_view().as_deref(), Some("files" | "countMatches"))
}

/// The shown length of a match value (`matchContentLength`).
fn content_length(q: MatchQuery<'_>) -> usize {
    q.match_content_length()
        .map_or(limits::MATCH_CONTENT_LENGTH_DEFAULT, |length| {
            length as usize
        })
        .clamp(1, limits::MATCH_CONTENT_LENGTH_MAXIMUM)
}

/// The size a match's row adds to its match page.
fn row_chars(row: &MatchRow) -> usize {
    octocode_engine::json_size::utf16_units(&row.value).unwrap_or(0) + 1
}

/// The row price of a match under `q`'s row shape.
fn match_price(q: MatchQuery<'_>, m: &StructuralDetailedMatch) -> u32 {
    let row = match_value(m, q.capture_text().unwrap_or(false), content_length(q));
    u32_of_usize(row_chars(&row))
}

/// What a directory scan keeps of each match: a file view needs only the
/// counts; a match view prices every row (pages are cut from the prices)
/// and rebuilds only the rows of the file page it shows.
fn demand<'a>(
    q: MatchQuery<'_>,
    price: &'a (dyn Fn(&StructuralDetailedMatch) -> u32 + Sync),
) -> StructuralDemand<'a> {
    if file_list(q) {
        StructuralDemand::Count
    } else {
        StructuralDemand::Priced(price)
    }
}

/// Rough heap bytes of a stored scan, its memo weight.
fn scan_bytes(scan: &Scan) -> usize {
    let diagnostic =
        |d: &StructuralDiagnostic| 64 + d.message.len() + d.path.as_ref().map_or(0, String::len);
    scan.files
        .iter()
        .map(|file| {
            96 + file.path.len()
                + file.source.as_ref().map_or(0, |(path, digest)| {
                    64 + path.as_os_str().len() + digest.len()
                })
                + match &file.matches {
                    FileMatches::Priced(prices) => prices.len() * 4,
                    FileMatches::Held(_) | FileMatches::Counted(_) => 0,
                }
                + file
                    .matches
                    .held()
                    .iter()
                    .map(|m| {
                        128 + m.text.len()
                            + m.header.as_ref().map_or(0, String::len)
                            + m.metavars
                                .values()
                                .flatten()
                                .map(|value| 24 + value.len())
                                .sum::<usize>()
                            + m.metavar_ranges
                                .values()
                                .flatten()
                                .map(|range| 48 + range.text.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
                + file.diagnostics.iter().map(diagnostic).sum::<usize>()
        })
        .sum::<usize>()
        + scan.diagnostics.iter().map(diagnostic).sum::<usize>()
        + scan.source_digest.len()
        + scan
            .sources
            .iter()
            .chain(&scan.discovery_sources)
            .map(|path| 64 + path.as_os_str().len())
            .sum::<usize>()
}

/// One file's row on the result page, with what its match page withheld.
struct Group {
    row: Value,
    /// What the file adds to a file page: its first match page.
    chars: usize,
    /// This match page starts past the file's last match.
    out_of_range: bool,
    more_matches: bool,
    truncated_captures: bool,
    /// The longest value this match page shortened, in characters.
    cut_chars: Option<usize>,
    /// Line spans of this match page's rows whose shown text was cut.
    clipped: Vec<(u64, u64)>,
    /// A priced file whose rows are not built yet: its index in the scan's
    /// files ([`rebuild_page`]).
    priced: Option<usize>,
}

/// How the candidate scan ended early, and where a resume starts.
#[derive(Clone, Copy)]
struct ScanCut<'a> {
    truncated: bool,
    resume: Option<u32>,
    /// What a cut at the walk ceiling left unread.
    unwalked: Option<&'a super::unwalked::Unwalked>,
}

/// Rows per file plus per-file coverage: files that parsed, files whose
/// query failed to compile, and files cut short (execution limit,
/// unreadable, unsupported).
#[derive(Default)]
struct Grouped {
    groups: Vec<Group>,
    /// The first file that parsed (its row path).
    first_parsed: Option<String>,
    /// The first file the scan read, parsed or skipped by the prefilter.
    first_scanned: Option<String>,
    diagnostics: Vec<Value>,
    total_matches: u64,
    parsed_files: u32,
    /// Files the scan read: parsed, or skipped by the literal-anchor prefilter.
    scanned_files: u32,
    compile_error: Option<ToolError>,
    compile_failed_files: u32,
    unevaluated_files: u32,
    /// Files skipped unread: no grammar, unreadable, over the parse limit.
    skipped_files: u32,
    /// The warning naming the grammars the query does not apply to.
    inapplicable_notice: Option<String>,
    /// Files whose recovered parse could hide a match.
    recovered: NamedFiles,
}

/// Diagnostic codes of a file the scan skipped unread.
const SKIP_CODES: [&str; 3] = [
    "structural.language.unsupported",
    "structural.file.unreadable",
    "structural.file.tooLarge",
];

pub fn execute_match(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> super::AstResult {
    cancel.check().map_err(ToolError::cancelled)?;
    let scope = resolve_scope(q, paths, cancel)?;
    let continues = q.page() > 1 || q.match_page() > 1;
    let key = scan_key(q, paths);
    // A continuation page reuses the scan its first page stored when nothing
    // it read changed; the snapshot below still decides whether it serves.
    let stored = q
        .snapshot()
        .filter(|_| continues)
        .and_then(|snapshot| SCANS.get(&snapshot, &key))
        .filter(|scan| {
            if !scan.untracked_ignore_dependencies && memo_source_unchanged(&scope, scan) {
                true
            } else {
                if let Some(snapshot) = q.snapshot() {
                    SCANS.evict(&snapshot);
                }
                false
            }
        });
    let fresh = stored.is_none();
    let (scan, started) = match stored {
        Some(scan) => (scan, octocode_engine::stamp::settle_clock()),
        None => {
            // Identical rows running at once (one batch) share one scan: the
            // first scans, the others take its scan from the flight's slot.
            let ticket = SCAN_FLIGHT.join(flight_key(q, &key));
            // Rows run on blocking workers, never on an async one. A waiter
            // stops on its own cancellation while another request scans.
            let mut slot = ticket
                .blocking_turn_until(|| cancel.check())
                .map_err(ToolError::cancelled)?;
            match slot.as_ref() {
                Some((scan, started)) => (std::sync::Arc::clone(scan), *started),
                None => {
                    let started = octocode_engine::stamp::settle_clock();
                    let mut scan = if scope.is_file {
                        scan_file(q, &scope)?
                    } else {
                        scan_directory(q, &scope, paths, cancel)?
                    };
                    cancel.check().map_err(ToolError::cancelled)?;
                    sort_files(q, &mut scan.files);
                    let scan = std::sync::Arc::new(scan);
                    // Only a scan that completed is shared; a failed one
                    // leaves the next row to scan itself.
                    *slot = Some((std::sync::Arc::clone(&scan), started));
                    (scan, started)
                }
            }
        }
    };
    let mut scope = scope;
    let grammars = compile_in_grammars(q, &scope, &scan);
    scope.inferred = infer_grammar(q, &scope, &scan, &grammars)?;
    let inapplicable = Inapplicable::of(&grammars);
    let snapshot = match_snapshot(q, &scope, &scan);
    // A page number without a snapshot asks for that page of a fresh scan;
    // only a snapshot that no longer matches is stale.
    if continues && q.snapshot().is_some_and(|given| given != snapshot) {
        if !fresh {
            // A stored scan is only a candidate: a page whose query no longer
            // matches it is answered by a fresh scan, exactly as without one.
            if let Some(stale) = q.snapshot() {
                SCANS.evict(&stale);
            }
            return execute_match(q, paths, cancel, response_window);
        }
        if let Some(error) = interrupted_rescan(q, &scan) {
            return Err(error);
        }
        return Ok(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
        ));
    }
    let budget = crate::response::page_windows::page_budget(response_window, &q.to_value());
    let mut grouped = group_files(q, &scan.files, budget, &inapplicable);
    grouped.inapplicable_notice = inapplicable.notice();
    // Group paths are rooted at the scope's parent; the notice names them as
    // the response names rows.
    grouped.recovered = grouped
        .recovered
        .respelled(|shown| super::response_path(paths, &scope.canonical, scope.is_file, shown));
    let shown = file_page(q, &grouped.groups, budget).window;
    if let Ok(incomplete) = rebuild_page(q, &scope, &scan.files, &mut grouped.groups[shown], budget)
    {
        // Counted and disclosed as the scan counts a file its limit cut.
        grouped.unevaluated_files += incomplete.len() as u32;
        grouped
            .diagnostics
            .extend(incomplete.iter().flatten().map(diag));
    } else {
        // A file on this page changed after the scan priced it.
        if !fresh {
            if let Some(stale) = q.snapshot() {
                SCANS.evict(&stale);
            }
            return execute_match(q, paths, cancel, response_window);
        }
        return Ok(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(ToolId::AstSearch, &q.to_value()),
        ));
    }
    if !scan.truncated
        && q.scan_offset() == 0
        && !grammars.is_empty()
        && grammars.iter().all(|grammar| grammar.outcome.is_err())
        && let Some(Err(error)) = grammars.first().map(|grammar| grammar.outcome.clone())
    {
        // Every grammar the whole scope holds (even in files the literal
        // prefilter left unparsed) rejects the query: only an initial
        // whole-scope scan can reject it for the selected set.
        return Err(ToolError::new(error.code, error.message));
    }
    if grouped.total_matches == 0
        && grouped.parsed_files == 0
        && grouped.compile_failed_files > 0
        && !scan.truncated
        && q.scan_offset() == 0
        && let Some(error) = grouped.compile_error
    {
        // Only a whole-scope scan proves there is no compatible candidate.
        // A capped incompatible window must retain coverage and scan onward.
        return Err(error);
    }
    let mut gaps = if scope.is_file {
        ScopeGaps::default()
    } else if grouped.total_matches == 0 && grouped.parsed_files == 0 && q.language().is_some() {
        ScopeGaps::of(q, &scope, paths, cancel)
    } else {
        // Files of the grammar behind skipped paths are still a reason,
        // beside the pattern's shape or the matches found: whether or not
        // the scan parsed any, and whether one language or each file's own
        // grammar selected them.
        ScopeGaps::beside_parsed(q, &scope, paths, cancel)
    };
    // The skipped-path probe walks within the scan's own file bound: a scan
    // cut at that bound cuts the probe too, and the scan's cut (with its
    // continuation) already says the result is partial.
    if scan.truncated {
        gaps.skipped = gaps.skipped.filter(|skipped| !skipped.disclosure.unchecked);
    }
    let exact_miss_scope = grouped.total_matches == 0
        && grouped.scanned_files > 0
        && !scan.truncated
        && q.scan_offset() == 0;
    let mut out = render_match_page(q, &scope, &scan, grouped, &snapshot, gaps, budget)?;
    if exact_miss_scope && let Some(miss) = exact_miss(q, &scope, paths, cancel) {
        miss.attach(&mut out);
    }
    if fresh
        && !scan.untracked_ignore_dependencies
        && out["next"]
            .as_object()
            .is_some_and(|next| next.contains_key("nextPage") || next.contains_key("nextMatchPage"))
    {
        let watched = scan
            .sources
            .iter()
            .chain(&scan.discovery_sources)
            .cloned()
            .collect::<Vec<_>>();
        SCANS.put(
            (snapshot, key),
            crate::tools::page_memo::Watch {
                root: &scope.canonical,
                sources: &watched,
                rules: &scan.ignore_rules,
                markers: &scan.repository_markers,
            },
            started,
            std::sync::Arc::clone(&scan),
            scan_bytes(&scan),
        );
    }
    Ok(out)
}

mod clipped;
mod render;
mod rows;
mod scan;
mod scope;
use clipped::*;
use render::*;
use rows::*;
use scan::*;
use scope::*;

#[cfg(test)]
mod language_glob_tests {
    fn language_include_globs(language: &str) -> Option<Vec<String>> {
        super::language_extensions(language).map(|extensions| {
            extensions
                .into_iter()
                .map(|extension| format!("*.{extension}"))
                .collect()
        })
    }

    #[test]
    fn directory_language_globs_derive_from_the_canonical_grammar_registry() {
        let cuda_enabled = octocode_engine::signatures::languages::grammar_capabilities()
            .iter()
            .any(|capability| {
                capability.structural_search && capability.extensions.iter().any(|ext| ext == "cu")
            });
        assert_eq!(
            language_include_globs("cuda"),
            cuda_enabled.then(|| vec!["*.cu".to_owned(), "*.cuh".to_owned()])
        );
        assert_eq!(
            language_include_globs("assembly"),
            Some(vec![
                "*.asm".to_owned(),
                "*.assembly".to_owned(),
                "*.s".to_owned()
            ])
        );
        assert_eq!(
            language_include_globs("typescript"),
            Some(vec![
                "*.cts".to_owned(),
                "*.mts".to_owned(),
                "*.ts".to_owned(),
                "*.tsx".to_owned()
            ])
        );
        assert_eq!(language_include_globs("unsupported"), None);
    }
}

#[cfg(test)]
mod bounded_text_tests {
    use super::bounded_match;

    #[test]
    fn exact_source_keeps_comments_line_breaks_and_indentation() {
        let go = "if err != nil { // The only error can be a bad pattern.\n\treturn fmt.Errorf(\"x\")\n}";
        assert_eq!(
            bounded_match(go, go.chars().count()),
            (go.to_owned(), false)
        );
        let rust = "builder\n    .worker_threads(1) // no timer!\n    .build()\n    .unwrap()";
        assert_eq!(
            bounded_match(rust, rust.chars().count()),
            (rust.to_owned(), false)
        );
        assert_eq!(bounded_match(rust, 12).0.chars().count(), 12);
    }
}

#[cfg(test)]
mod memo_tests {
    use super::super::test_rows::run_row;
    use super::*;
    use crate::tools::test_support::{Fixture, simple_policy};

    fn next_query(out: &Value, name: &str) -> Value {
        out["next"][name]["query"]["queries"][0].clone()
    }

    /// A continuation page served from the first page's stored scan is
    /// byte-identical to the page a fresh scan gives; an edit drops the
    /// stored scan and the page restarts as before.
    #[test]
    fn continuation_pages_reuse_the_settled_scan_with_identical_output() {
        let root = Fixture::new();
        for (name, calls) in [("a.ts", 3), ("b.ts", 1), ("c.ts", 2)] {
            let body = (0..calls)
                .map(|i| format!("console.log({i});\n"))
                .collect::<String>();
            std::fs::write(root.0.join(name), body).expect("file");
        }
        std::fs::write(root.0.join("d.ts"), "other();\n").expect("prefilter skip");
        let (paths, security) = simple_policy(&root.0);
        octocode_engine::stamp::settle_written_so_far();
        let first = run_row(
            &json!({"operation":"match","path":root.0,"pattern":"console.log($A)","language":"typescript","pageSize":1,"matchPageSize":2}),
            &paths,
            &security,
        );
        let snapshot = first["snapshot"].as_str().expect("snapshot").to_owned();
        assert!(
            SCANS.holds(&snapshot),
            "a multi-page scan is stored: {first}"
        );
        let mut offering = first.clone();
        let mut file_page_query = Value::Null;
        for name in ["nextMatchPage", "nextPage"] {
            let query = next_query(&offering, name);
            assert!(query.is_object(), "{name}: {offering}");
            if name == "nextPage" {
                file_page_query = query.clone();
            }
            let stored = run_row(&query, &paths, &security);
            SCANS.evict(&snapshot);
            let fresh = run_row(&query, &paths, &security);
            assert_eq!(
                serde_json::to_string(&stored).expect("json"),
                serde_json::to_string(&fresh).expect("json"),
                "{name}"
            );
            // The rescan stores the scan again for the pages after it.
            assert!(SCANS.holds(&snapshot));
            offering = fresh;
        }
        // An edit drops the stored scan: the page restarts from page 1.
        std::fs::write(root.0.join("c.ts"), "console.log(9);\n").expect("edit");
        let page = run_row(&file_page_query, &paths, &security);
        assert_eq!(page["errorCode"], "staleSnapshot", "{page}");
        assert!(!SCANS.holds(&snapshot));
    }
}

#[cfg(test)]
mod interrupted_rescan_tests {
    use super::*;
    use octocode_contract_types::ErrorCode;

    /// A rescan cut by the match deadline (zero matches, status truncated)
    /// is a retryable timeout for the same page, not a stale snapshot; a
    /// complete rescan that differs stays stale.
    #[test]
    fn a_deadline_cut_rescan_is_a_retryable_timeout() {
        let query: AstSearchQueryMatchPattern = serde_json::from_value(json!({
            "operation":"match","path":"/repo/a.min.js","pattern":"f($A)",
            "matchPage":9,"snapshot":"earlier"
        }))
        .expect("typed row");
        let q = MatchQuery::Pattern(&query);
        let file = |diagnostics, status: &str| ScannedFile {
            path: "a.min.js".to_owned(),
            parser: String::new(),
            matches: FileMatches::Held(vec![]),
            diagnostics,
            status: status.to_owned(),
            recovered: false,
            source: None,
        };
        let cut = Scan {
            files: vec![file(
                vec![StructuralDiagnostic {
                    code: "structural.match.deadline".to_owned(),
                    severity: "warning".to_owned(),
                    stage: "match".to_owned(),
                    message: "Structural matching exceeded its execution deadline".to_owned(),
                    path: None,
                    recovery: None,
                }],
                "truncated",
            )],
            ..Scan::default()
        };
        let error = interrupted_rescan(q, &cut).expect("a deadline cut is a timeout");
        assert_eq!(error.code, ErrorCode::Timeout);
        let retry = &error.next.expect("retry")["retry"]["query"]["queries"][0];
        assert_eq!(retry["matchPage"], 9);
        assert_eq!(retry["snapshot"], "earlier");
        let changed = Scan {
            files: vec![file(vec![], "ok")],
            ..Scan::default()
        };
        assert!(interrupted_rescan(q, &changed).is_none());
    }
}

#[cfg(test)]
mod source_freshness_tests;

#[cfg(test)]
mod discovery_freshness_tests;

#[cfg(test)]
mod external_ignore_freshness_tests;

#[cfg(test)]
mod flight_tests;

#[cfg(test)]
mod demand_tests;
