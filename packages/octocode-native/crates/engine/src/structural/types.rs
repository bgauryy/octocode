use crate::text::analysis::{ObservedSpan, OriginProjection, SourceDescriptors, SourceTemplates};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Precise position of one captured metavariable node. Line is 1-based (usable
/// as an `lspSearch` `line`); columns are 0-based UTF-16 code-unit
/// offsets — the same convention as `StructuralMatch.start_col` and LSP.
pub struct MetavarRange {
    pub source_span: ObservedSpan,
    pub source_origin: Option<OriginProjection>,
    pub text: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// One structural match. Line numbers are 1-based so `start_line` can be fed
/// directly as an `lspSearch` `line`; columns are 0-based UTF-16
/// code-unit offsets (converted from tree-sitter's native byte columns to match
/// the resolver, signatures, and LSP layers).
pub(crate) struct StructuralMatch {
    pub source_span: ObservedSpan,
    pub header_span: Option<ObservedSpan>,
    pub source_templates: Arc<SourceTemplates>,
    pub source_origin: Option<OriginProjection>,
    pub header_origin: Option<OriginProjection>,
    pub source_descriptors: Option<Arc<SourceDescriptors>>,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub text: String,
    /// Captured metavariables. `$X` yields a single-element list;
    /// `$$$ARGS` yields the full list of captured nodes. Keyed by the bare
    /// metavar name (no leading `$`).
    pub metavars: HashMap<String, Vec<String>>,
    /// Per-capture precise ranges, parallel to `metavars` (same keys, same
    /// order). Lets an agent hand a capture straight to LSP without re-search.
    pub metavar_ranges: BTreeMap<String, Vec<MetavarRange>>,
    /// Source before the node's `body` block when the node spans several
    /// lines (declaration signature, loop head); `None` without a body field.
    pub header: Option<String>,
}

#[derive(Clone)]
pub struct StructuralDiagnostic {
    pub code: String,
    pub severity: String,
    pub stage: String,
    pub message: String,
    pub path: Option<String>,
    pub recovery: Option<String>,
}

pub struct StructuralQueryExplanation {
    pub kind: String,
    pub source: String,
    pub literal_anchor: Option<String>,
    pub pre_filter: String,
    pub unsafe_reason: Option<String>,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

/// A structural match plus the kind of the matched node, for the detailed
/// APIs.
pub struct StructuralDetailedMatch {
    pub source_span: ObservedSpan,
    pub header_span: Option<ObservedSpan>,
    pub source_templates: Arc<SourceTemplates>,
    pub source_origin: Option<OriginProjection>,
    pub header_origin: Option<OriginProjection>,
    pub source_descriptors: Option<Arc<SourceDescriptors>>,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub text: String,
    pub metavars: HashMap<String, Vec<String>>,
    pub metavar_ranges: BTreeMap<String, Vec<MetavarRange>>,
    pub header: Option<String>,
    pub node_kind: Option<String>,
}

/// What a file search keeps of each file's matches ([`super::search_files`]).
/// Every demand finds the same matches and counts them exactly; they differ
/// only in what each match leaves behind.
#[derive(Clone, Copy)]
pub enum StructuralDemand<'a> {
    /// Every match, materialized ([`FileMatches::Held`]).
    Matches,
    /// Each file's match count ([`FileMatches::Counted`]): no match text,
    /// capture or header is built.
    Count,
    /// Each match handed to the caller's price as it is found and then
    /// dropped ([`FileMatches::Priced`]): a file keeps one price per match,
    /// in match order, and a page rebuilds the matches it shows
    /// ([`super::file_matches`]).
    Priced(&'a (dyn Fn(&StructuralDetailedMatch) -> u32 + Sync)),
    /// [`Self::Priced`], and the window ends at a start of one of the
    /// caller's pages ([`PagedWindow`]): a window bounded by the pages it
    /// shows, not by its scope, each of them whole.
    Paged(PagedWindow<'a>),
    /// Whether the scope holds any match: each file's count, as
    /// [`Self::Count`], and the scan ends at the first chunk that holds a
    /// match, resuming after it ([`super::search_files`]'s `scan_resume`).
    Exists,
}

/// A [`StructuralDemand::Paged`] window: each match priced, and the window
/// ends at the first start of a page of the caller's past page 1 that lies
/// at least as many candidates in as the windows before it read, once a
/// match lies past it in another file (with none, the window ends with the
/// scope), so a walk takes a number of windows logarithmic in its scope and
/// every page a window shows is whole. A start inside a file ends the window
/// with that file ([`StructuralSearchFilesDetailedResult::page_split`]): the
/// next window, resumed with `resumes_inside`, reads it again and lists its
/// matches from there. A window its time cuts ends at the last page start it
/// met, else at its time.
#[derive(Clone, Copy)]
pub struct PagedWindow<'a> {
    pub price: &'a (dyn Fn(&StructuralDetailedMatch) -> u32 + Sync),
    pub page_starts: &'a PageStarts<'a>,
    /// The window before ended inside candidate `skip_files - 1`: this
    /// window evaluates it again, first.
    pub resumes_inside: bool,
}

/// Where the caller's pages start among the files a
/// [`StructuralDemand::Paged`] window evaluated: called once per chunk,
/// serially, with that chunk's evaluated files in candidate order, each with
/// its position from the window's first candidate (a caller keeps what
/// earlier chunks held, so it sees every file once, in walk order). It
/// returns each start of a page past the first that these files hold, in
/// order, as (position, index of the page's first match in that file's
/// matches).
pub type PageStarts<'a> =
    dyn Fn(&[(usize, &StructuralSearchDetailedFileResult)]) -> Vec<(usize, usize)> + Sync + 'a;

/// What a file search kept of one file's matches, by its
/// [`StructuralDemand`].
pub enum FileMatches {
    Held(Vec<StructuralDetailedMatch>),
    Priced(Vec<u32>),
    Counted(u32),
}

impl FileMatches {
    /// How many matches the file has, whatever was kept of them.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Held(matches) => matches.len(),
            Self::Priced(prices) => prices.len(),
            Self::Counted(count) => *count as usize,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The materialized matches; empty unless they were held.
    #[must_use]
    pub fn held(&self) -> &[StructuralDetailedMatch] {
        match self {
            Self::Held(matches) => matches,
            Self::Priced(_) | Self::Counted(_) => &[],
        }
    }
}

#[derive(Default)]
pub struct StructuralSearchFilesOptions {
    pub path: String,
    pub pattern: Option<String>,
    pub rule: Option<serde_json::Value>,
    pub include: Option<Vec<String>>,
    /// File-path globs to skip (gitignore-style, e.g. `"*.min.js"`, `"src/gen/**"`).
    /// Mirrors local-search `exclude`.
    pub exclude: Option<Vec<String>>,
    /// Default-excluded directory names (see [`Prune::at`]): pruned outside a
    /// git work tree, and inside one only where git ignores them.
    ///
    /// [`Prune::at`]: crate::search::walk::Prune::at
    pub default_exclude_dir: Option<Vec<String>>,
    /// Which ignore rules and dot entries the walk leaves out.
    pub ignore: crate::types::IgnorePolicy,
    /// Maximum path depth below a directory root (1 = direct files; 0 = no descent). `None` = unbounded.
    pub max_depth: Option<u32>,
    pub max_files: Option<u32>,
    /// Leading candidates (walk order) left unevaluated: an earlier scan of
    /// the same scope already evaluated them. They still count toward
    /// `max_files`.
    pub skip_files: Option<u32>,
    pub max_file_bytes: Option<u32>,
    /// The text each source is matched as, in place of its admitted text
    /// ([`TextJudge`]); `None` matches every source as admitted.
    pub judge: Option<TextJudge>,
    /// The global ignore rules of the call, which the walk applies per
    /// repository.
    pub global_ignore: crate::search::walk::GlobalIgnore,
    /// The caller's key for keeping each file's outcome across scans: its
    /// demand's price is a function of the match and this key alone. `None`
    /// keeps nothing.
    pub kept: Option<String>,
}

/// What a structural search hides of a source whose admitted text is
/// `text`: each byte range of `text` its judge hides, with the text a read
/// shows there (empty: nothing hidden). The source is matched as the text a
/// read shows ([`crate::text::analysis::AdmittedSource::judged`]), so a
/// node's lines, columns and byte ranges count that text, as a read of it
/// does, and a match, a count, a `regex`, `range`, `follows` or `precedes`
/// decision, a digest or a byte range never reads a byte a read withholds,
/// nor its length.
pub type JudgeText =
    dyn Fn(&std::path::Path, &str) -> Vec<(std::ops::Range<usize>, String)> + Send + Sync;

/// A shared [`JudgeText`].
#[derive(Clone)]
pub struct TextJudge(pub Arc<JudgeText>);

impl std::fmt::Debug for TextJudge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TextJudge")
    }
}

/// A macro body whose re-parse could not read a node the query matched
/// (recovered syntax), so a match inside it may be missing: the macro
/// (`json!`, `#define NAME`) and the 1-based line its text starts on.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecoveredBody {
    pub name: String,
    pub line: u32,
}

pub struct StructuralSearchDetailedResult {
    pub path: String,
    pub status: String,
    pub language_id: Option<String>,
    pub query: StructuralQueryExplanation,
    pub matches: Vec<StructuralDetailedMatch>,
    /// The source parsed with recovered syntax that could hide a match: its
    /// matches may be incomplete.
    pub recovered: bool,
    /// Macro bodies whose re-parse could hide a match, in source order.
    pub recovered_bodies: Vec<RecoveredBody>,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

pub struct StructuralSearchDetailedFileResult {
    pub path: String,
    /// Digest of the complete source bytes already read for this scan.
    /// None when no complete source identity was available.
    pub source_sha256: Option<String>,
    pub status: String,
    pub language_id: Option<String>,
    pub skipped_reason: Option<String>,
    pub matches: FileMatches,
    /// The file parsed with recovered syntax that could hide a match: its
    /// matches may be incomplete.
    pub recovered: bool,
    /// Macro bodies whose re-parse could hide a match, in source order.
    pub recovered_bodies: Vec<RecoveredBody>,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

pub struct StructuralSearchFilesDetailedResult {
    pub scan_truncated: bool,
    /// The scan stopped at a start of a [`StructuralDemand::Paged`] page,
    /// not at its time budget: what it skipped is pagination, not a gap.
    pub page_filled: bool,
    /// The page start the window ended at is inside its last evaluated file:
    /// its pages hold this many of that file's matches, and the window
    /// resumed with [`PagedWindow::resumes_inside`] lists the rest.
    pub page_split: Option<u32>,
    /// The file the window before ended inside was evaluated again: it is
    /// the first of `files`.
    pub resumed_inside: bool,
    /// Where a truncated scan resumes: the walk-order index of the first
    /// unevaluated candidate (pass it back as `skip_files`). `None` when the
    /// scan is complete, or was cut by an explicit `max_files`.
    pub scan_resume: Option<u32>,
    /// What the candidates `skip_files` passed are now
    /// ([`crate::digest::ListedFiles`]); `None` when it passed none.
    pub listed_before: Option<String>,
    /// The same through the resume position, what the resume compares it with.
    pub listed_through: Option<String>,
    /// The candidates `skip_files` passed.
    pub earlier_files: Vec<std::path::PathBuf>,
    pub files: Vec<StructuralSearchDetailedFileResult>,
    pub total_matches: u32,
    pub parsed_files: u32,
    pub skipped_by_pre_filter: u32,
    pub skipped_unsupported: u32,
    pub skipped_unreadable: u32,
    pub skipped_large: u32,
    /// Entries the directory walk could not read (unreadable directories,
    /// entries it could not stat), through the last candidate it collected:
    /// absence below them is unproven.
    pub walk_errors: WalkErrors,
    /// What the walk's own rules left out, through the last candidate it
    /// collected: nothing behind them was read.
    pub walk_skipped: WalkSkipped,
    pub status: String,
    pub query: StructuralQueryExplanation,
    pub diagnostics: Vec<StructuralDiagnostic>,
    pub warnings: Vec<String>,
}

/// Which matches a line scope keeps, by file and 1-based start line.
pub type KeepLine<'a> = dyn Fn(&std::path::Path, u32) -> bool + Sync + 'a;

/// What a structural walk's own rules left out, each entry by its path under
/// the walk root with `/` separators, a directory's with a trailing `/` (its
/// contents were never walked), in walk order: an ignore rule's entries
/// (`.gitignore`, `.ignore`, git excludes, global rules), dot entries without
/// `includeHidden`, and the directories the default prune skipped. The
/// topmost skipped entry of each subtree is named, never what it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalkSkipped {
    pub ignored: Vec<String>,
    pub hidden: Vec<String>,
    pub pruned: Vec<String>,
}

impl WalkSkipped {
    /// Whether the walk's rules left nothing out.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ignored.is_empty() && self.hidden.is_empty() && self.pruned.is_empty()
    }
}

/// What a structural walk could not read: the count (`skipped` and
/// `permission_denied`, as every other walk counts them, so one disclosure
/// words them) and the first few errors, each naming its path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalkErrors {
    pub skipped: u32,
    pub permission_denied: u32,
    pub sample: Vec<String>,
}

/// Errors [`WalkErrors::sample`] keeps.
const WALK_ERROR_SAMPLE: usize = 3;

impl WalkErrors {
    pub(super) fn record(&mut self, error: &ignore::Error) {
        self.note(Self::denied(error), &error.to_string());
    }

    /// Whether `error` is a denied permission.
    pub(super) fn denied(error: &ignore::Error) -> bool {
        error
            .io_error()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
    }

    /// Count one entry the walk could not read, as [`Self::record`] counts
    /// an error that `denied` permission or not and reads as `message`.
    pub(super) fn note(&mut self, denied: bool, message: &str) {
        self.skipped = self.skipped.saturating_add(1);
        if denied {
            self.permission_denied = self.permission_denied.saturating_add(1);
        }
        if self.sample.len() < WALK_ERROR_SAMPLE {
            self.sample.push(message.to_owned());
        }
    }
}

impl StructuralDiagnostic {
    /// The warning on a file read as legacy 8-bit text ([`crate::text::decode`]):
    /// matches and positions count its decoded text.
    #[must_use]
    pub fn decoded(path: &str, warning: &str) -> Self {
        Self::new("structural.source.decoded", "warning", "source", warning)
            .with_path(path.to_owned())
    }

    pub(super) fn new(
        code: impl Into<String>,
        severity: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: severity.into(),
            stage: stage.into(),
            message: message.into(),
            path: None,
            recovery: None,
        }
    }

    pub(super) fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub(super) fn with_recovery(mut self, recovery: impl Into<String>) -> Self {
        self.recovery = Some(recovery.into());
        self
    }
}

impl StructuralDetailedMatch {
    pub(super) fn from_match(matched: StructuralMatch, node_kind: impl Into<String>) -> Self {
        Self {
            source_span: matched.source_span,
            header_span: matched.header_span,
            source_templates: matched.source_templates,
            source_origin: matched.source_origin,
            header_origin: matched.header_origin,
            source_descriptors: matched.source_descriptors,
            start_line: matched.start_line,
            end_line: matched.end_line,
            start_col: matched.start_col,
            end_col: matched.end_col,
            text: matched.text,
            metavars: matched.metavars,
            metavar_ranges: matched.metavar_ranges,
            header: matched.header,
            node_kind: Some(node_kind.into()),
        }
    }
}
