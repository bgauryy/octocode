use octocode_engine::structural::FileMatches;
use octocode_engine::structural::StructuralDiagnostic;
use octocode_tools_shared::ast_rule::GlobParsers;
use octocode_tools_shared::leads::unparsed::Reader;
pub(super) use octocode_tools_shared::leads::unparsed::{NAMED_EXAMPLES, NamedFiles, Unparsed};
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
    /// Without `parser`: the parser extensions of the scan's own census whose
    /// grammar the query compiles in ([`super::scope::read_grammars`]).
    pub(super) compiled: Vec<String>,
}

/// One scanned file.
pub(super) struct ScannedFile {
    /// The row path.
    pub(super) path: String,
    /// The parser extension the file was read with (empty for a single file,
    /// which no grammar census covers).
    pub(super) parser: String,
    /// What the scan kept of its matches (see [`super::demand`]).
    pub(super) matches: FileMatches,
    pub(super) diagnostics: Vec<StructuralDiagnostic>,
    /// The engine status.
    pub(super) status: String,
    /// The file parsed with recovered syntax that could hide a match.
    pub(super) recovered: bool,
    /// Macro bodies whose re-parse could hide a match.
    pub(super) recovered_bodies: Vec<octocode_engine::structural::RecoveredBody>,
    /// The scanned path and the digest of the bytes it was matched in:
    /// where a page rebuilds a priced file's matches.
    pub(super) source: Option<(std::path::PathBuf, String)>,
}

/// The files a scan evaluated plus its corpus-coverage signals. A
/// single-file scan leaves the signals at their complete defaults.
#[derive(Default)]
pub(super) struct Scan {
    pub(super) files: Vec<ScannedFile>,
    /// The window did not reach the end of the scope: its time budget cut
    /// it, or its page filled (`filled`).
    pub(super) truncated: bool,
    /// The window stopped because its rows filled more than page 1
    /// ([`super::scan::Keeps::Page`]): what it left is pagination (its
    /// totals are floors), not a coverage gap.
    pub(super) filled: bool,
    /// The window ended at a page start inside its last file: its stream
    /// holds this many of that file's matches, the next window the rest.
    pub(super) page_split: Option<u32>,
    /// The window read again the file the window before ended inside: it is
    /// the first of `files`, and its stream starts past what that window
    /// listed ([`super::snapshot::WindowStart::carry`]).
    pub(super) resumed_inside: bool,
    /// Where a cut resumes (`scanOffset`), in walk order.
    pub(super) scan_resume: Option<u32>,
    /// What the candidates before this window are now, and through its cut
    /// ([`octocode_engine::digest::ListedFiles`]).
    pub(super) listed_before: Option<String>,
    pub(super) listed_through: Option<String>,
    /// The candidates before this window: its pages answer for them too.
    pub(super) earlier_files: Vec<std::path::PathBuf>,
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
    /// The directories the walk entered, not source reads.
    pub(super) discovery_sources: Vec<std::path::PathBuf>,
    /// Files outside the walk whose contents decide its ignore rules, and
    /// the repository markers whose presence does (empty when ignore rules
    /// are off): the memo watches them like the walked entries.
    pub(super) ignore_rules: Vec<std::path::PathBuf>,
    pub(super) repository_markers: Vec<std::path::PathBuf>,
    /// Ignore files take part in the walk: keeping the scan watches those of
    /// its directories too.
    pub(super) watches_ignore_files: bool,
    /// What the walk left out: what its ignore, dot and default-exclude
    /// rules skipped of what the scan reads (nothing behind them was read),
    /// the entries the security path policy withheld, and Octocode's own
    /// store directories.
    pub(super) skipped: octocode_runtime_base::policy::discovery::WalkSkips,
    /// Files the walk passed over because no grammar parses their names.
    pub(super) unparsed: Unparsed,
    /// Without `parser`: the files the walk admitted, by parser extension
    /// (the census the scope's grammar is inferred from).
    pub(super) census: std::collections::BTreeMap<String, usize>,
    /// Under `parser`: every file the walk met that the path policy admits, by
    /// extension, parsed or not (the census of the grammars it could offer).
    pub(super) present: Option<std::collections::BTreeMap<String, usize>>,
    /// What a `testCode` scope left out.
    pub(super) test_code_excluded: octocode_engine::types::TestCodeExcluded,
    /// Of `test_code_excluded.files`, those the walk left unread by where they
    /// lie: a window's walk meets every file the windows before it met, so
    /// these are counted again by each window, while the rest are the files
    /// this window read.
    pub(super) test_code_walked: u32,
    /// Rust files a `testCode` verdict needed and could not read.
    pub(super) test_code_unread: Vec<std::path::PathBuf>,
}

/// astSearch's reading of a file no grammar parses: it was not searched.
pub(super) const SEARCH: Reader = Reader {
    grammar: "AST grammar",
    unread: [
        "it was not searched, so absence there is unproven",
        "they were not searched, so absence there is unproven",
    ],
};

/// Macro bodies, by file, line and macro, whose re-parse could hide a
/// match: how many, and the first few.
#[derive(Default)]
pub(super) struct NamedBodies {
    pub(super) count: u32,
    pub(super) examples: std::collections::BTreeSet<(String, u32, String)>,
}

impl NamedBodies {
    pub(super) fn record(&mut self, path: &str, body: &octocode_engine::structural::RecoveredBody) {
        self.count = self.count.saturating_add(1);
        self.examples
            .insert((path.to_owned(), body.line, body.name.clone()));
        if self.examples.len() > NAMED_EXAMPLES {
            self.examples.pop_last();
        }
    }

    /// The same tally with each path respelled by `spell`.
    pub(super) fn respelled(&self, spell: impl Fn(&str) -> String) -> Self {
        Self {
            count: self.count,
            examples: self
                .examples
                .iter()
                .map(|(path, line, name)| (spell(path), *line, name.clone()))
                .collect(),
        }
    }
}

/// The warning that names the macro bodies whose re-parse could hide a
/// match: `path:line macro!`, the first few.
pub(super) fn recovered_bodies_notice(bodies: &NamedBodies) -> Option<String> {
    let noun = match bodies.count {
        0 => return None,
        1 => "macro body",
        _ => "macro bodies",
    };
    let mut names: Vec<String> = bodies
        .examples
        .iter()
        .map(|(path, line, name)| format!("{path}:{line} {name}"))
        .collect();
    if bodies.count as usize > names.len() {
        names.push("…".to_owned());
    }
    Some(format!(
        "{} {noun} re-parsed with recovered syntax that could hide a match ({}); matches inside them may be missing.",
        bodies.count,
        names.join(", ")
    ))
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

/// One file's row on the result page, with what its matches withheld.
pub(super) struct Group {
    pub(super) row: Value,
    pub(super) source_descriptors:
        Vec<std::sync::Arc<octocode_engine::text::analysis::SourceDescriptors>>,
    /// In the match views, what the file's header adds to each page its
    /// matches are on; in the file views, the file's row.
    pub(super) chars: usize,
    /// Each match's row size, in walk order ([`super::stream`]); empty in
    /// the file views, where the file is the paged item.
    pub(super) sizes: Vec<usize>,
    /// A held scan's rows, sliced when a page shows them.
    pub(super) held: Option<Vec<super::rows::MatchRow>>,
    pub(super) truncated_captures: bool,
    /// A priced file whose rows are not built yet: its index in the scan's
    /// files ([`super::groups::fill_page`]).
    pub(super) priced: Option<usize>,
}

/// How the candidate scan ended early, and where a resume starts.
#[derive(Clone, Copy)]
pub(super) struct ScanCut<'a> {
    pub(super) truncated: bool,
    /// The window ended because its page filled ([`Scan::filled`]): the
    /// next window is the walk's next page.
    pub(super) filled: bool,
    pub(super) resume: Option<u32>,
    /// What the resume pins of the files through the cut.
    pub(super) pin: &'a str,
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
    pub(super) unevaluated_files: u32,
    /// Files skipped unread: no grammar, unreadable, over the parse limit.
    pub(super) skipped_files: u32,
    /// Files per grammar the query does not compile in.
    pub(super) skipped_grammars: Option<Value>,
    /// Files whose recovered parse could hide a match.
    pub(super) recovered: NamedFiles,
    /// Macro bodies whose recovered re-parse could hide a match.
    pub(super) recovered_bodies: NamedBodies,
}

/// Diagnostic codes of a file the scan skipped unread.
pub(super) const SKIP_CODES: [&str; 3] = [
    "structural.language.unsupported",
    "structural.file.unreadable",
    "structural.file.tooLarge",
];
