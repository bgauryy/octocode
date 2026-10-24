use crate::response::result::ToolError;
use crate::tools::ast_rule::GlobParsers;
use octocode_engine::structural::FileMatches;
use octocode_engine::structural::StructuralDiagnostic;
use serde_json::Value;

/// The validated scope of one `match` query and the grammar it runs with.
pub(super) struct Scope {
    pub(super) canonical: std::path::PathBuf,
    pub(super) is_file: bool,
    /// An explicit parser selector; otherwise each file selects its grammar.
    pub(super) language: Option<String>,
    /// The grammar observed when the discovery window holds only one.
    pub(super) inferred: Option<String>,
    pub(super) extensions: Option<std::collections::BTreeSet<String>>,
    /// `parserGlobs`: the parser of files whose names select none.
    pub(super) parsers: GlobParsers,
}

/// One scanned file.
pub(super) struct ScannedFile {
    /// The row path.
    pub(super) path: String,
    /// The parser extension the file was read with (empty for a single file,
    /// which no grammar census covers).
    pub(super) parser: String,
    /// What the scan kept of its matches (see [`demand`]).
    pub(super) matches: FileMatches,
    pub(super) diagnostics: Vec<StructuralDiagnostic>,
    /// The engine status.
    pub(super) status: String,
    /// The file parsed with recovered syntax that could hide a match.
    pub(super) recovered: bool,
    /// The scanned path and the digest of the bytes it was matched in:
    /// where a page rebuilds a priced file's matches.
    pub(super) source: Option<(std::path::PathBuf, String)>,
}

/// The files a scan evaluated plus its corpus-coverage signals. A
/// single-file scan leaves the signals at their complete defaults.
#[derive(Default)]
pub(super) struct Scan {
    pub(super) files: Vec<ScannedFile>,
    pub(super) truncated: bool,
    /// Where a time-budget cut resumes (`scanOffset`), in walk order.
    pub(super) scan_resume: Option<u32>,
    pub(super) diagnostics: Vec<StructuralDiagnostic>,
    /// Unsupported, unreadable and over-large files skipped.
    pub(super) skips: (u32, u32, u32),
    /// Entries the walk could not read (an unreadable directory): absence
    /// below them is unproven.
    pub(super) walk_errors: octocode_engine::structural::WalkErrors,
    pub(super) skipped_by_prefilter: u32,
    /// Every file the scan evaluated (absolute), for the memo's stamps.
    pub(super) sources: Vec<std::path::PathBuf>,
    /// Canonical digest of complete source bytes already read by this scan.
    pub(super) source_digest: String,
    /// Directory and ignore-file metadata observed during discovery, not source reads.
    pub(super) discovery_sources: Vec<std::path::PathBuf>,
    /// Files outside the walk whose contents decide its ignore rules, and
    /// the repository markers whose presence does (empty when ignore rules
    /// are off): the memo watches them like the walked entries.
    pub(super) ignore_rules: Vec<std::path::PathBuf>,
    pub(super) repository_markers: Vec<std::path::PathBuf>,
    /// An ignore dependency no stamp can watch (a link, an unreadable
    /// entry) disables reuse.
    pub(super) untracked_ignore_dependencies: bool,
    /// Candidate entries the security path policy withheld from the walk.
    pub(super) withheld: crate::policy::discovery::Withheld,
    /// Files the walk passed over because no grammar parses their names.
    pub(super) unparsed: NamedFiles,
    /// Without `parser`: the files the walk admitted, by parser extension
    /// (the census the scope's grammar is inferred from).
    pub(super) census: std::collections::BTreeMap<String, usize>,
    /// The scopes a scan cut at the walk ceiling never read.
    pub(super) unwalked: Option<crate::tools::ast_search::unwalked::Unwalked>,
}

/// Example paths a file-count warning names.
pub(super) const NAMED_EXAMPLES: usize = 3;

/// A tally of files a warning names: their count and the first
/// [`NAMED_EXAMPLES`] paths in path order, spelled as the response's row
/// paths are (workspace-relative, else absolute).
#[derive(Default)]
pub(super) struct NamedFiles {
    pub(super) count: u32,
    pub(super) examples: std::collections::BTreeSet<String>,
}

impl NamedFiles {
    pub(super) fn record(&mut self, path: String) {
        self.count = self.count.saturating_add(1);
        self.examples.insert(path);
        if self.examples.len() > NAMED_EXAMPLES {
            self.examples.pop_last();
        }
    }

    /// The same tally with each path respelled by `spell`.
    pub(super) fn respelled(&self, spell: impl Fn(&str) -> String) -> Self {
        Self {
            count: self.count,
            examples: self.examples.iter().map(|path| spell(path)).collect(),
        }
    }

    /// The paths a warning lists: `a, b, c, …` when more files follow.
    pub(super) fn listed(&self) -> String {
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
pub(super) fn unparsed_notice(files: &NamedFiles) -> Option<String> {
    let names = files.listed();
    match files.count {
        0 => None,
        1 => Some(format!(
            "1 file skipped: no AST grammar for its name ({names}); if it is source code, parserGlobs maps its name to a grammar."
        )),
        count => Some(format!(
            "{count} files skipped: no AST grammar for their names ({names}); if they are source code, parserGlobs maps their names to a grammar."
        )),
    }
}

/// The warning that names the files whose recovered parse could hide a
/// match (see `octocode_engine::structural`'s recovery criterion): the
/// matches listed are not all of the file's.
pub(super) fn recovered_notice(files: &NamedFiles) -> Option<String> {
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

/// One file's row on the result page, with what its match page withheld.
pub(super) struct Group {
    pub(super) row: Value,
    /// What the file adds to a file page: its first match page.
    pub(super) chars: usize,
    /// This match page starts past the file's last match.
    pub(super) out_of_range: bool,
    pub(super) more_matches: bool,
    pub(super) truncated_captures: bool,
    /// The longest value this match page shortened, in characters.
    /// Line spans of this match page's rows whose shown text was cut.
    pub(super) clipped: Vec<(u64, u64)>,
    /// A priced file whose rows are not built yet: its index in the scan's
    /// files ([`rebuild_page`]).
    pub(super) priced: Option<usize>,
}

/// How the candidate scan ended early, and where a resume starts.
#[derive(Clone, Copy)]
pub(super) struct ScanCut<'a> {
    pub(super) truncated: bool,
    pub(super) resume: Option<u32>,
    /// What a cut at the walk ceiling left unread.
    pub(super) unwalked: Option<&'a crate::tools::ast_search::unwalked::Unwalked>,
}

/// Rows per file plus per-file coverage: files that parsed, files whose
/// query failed to compile, and files cut short (execution limit,
/// unreadable, unsupported).
#[derive(Default)]
pub(super) struct Grouped {
    pub(super) groups: Vec<Group>,
    /// The first file that parsed (its row path).
    pub(super) first_parsed: Option<String>,
    pub(super) diagnostics: Vec<Value>,
    pub(super) total_matches: u64,
    pub(super) parsed_files: u32,
    /// Files the scan read: parsed, or skipped by the literal-anchor prefilter.
    pub(super) scanned_files: u32,
    pub(super) compile_error: Option<ToolError>,
    pub(super) compile_failed_files: u32,
    pub(super) unevaluated_files: u32,
    /// Files skipped unread: no grammar, unreadable, over the parse limit.
    pub(super) skipped_files: u32,
    /// Files per grammar the query does not compile in.
    pub(super) skipped_grammars: Option<Value>,
    /// Files whose recovered parse could hide a match.
    pub(super) recovered: NamedFiles,
}

/// Diagnostic codes of a file the scan skipped unread.
pub(super) const SKIP_CODES: [&str; 3] = [
    "structural.language.unsupported",
    "structural.file.unreadable",
    "structural.file.tooLarge",
];
