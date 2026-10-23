use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// Precise position of one captured metavariable node. Line is 1-based (usable
/// as an `lspSearch` `line`); columns are 0-based UTF-16 code-unit
/// offsets — the same convention as `StructuralMatch.start_col` and LSP.
pub struct MetavarRange {
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
pub struct StructuralMatch {
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
}

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
    pub exclude_dir: Option<Vec<String>>,
    /// Default-excluded directory names (see [`Prune::at`]): pruned outside a
    /// git work tree, and inside one only where git ignores them.
    ///
    /// [`Prune::at`]: crate::search::walk::Prune::at
    pub default_exclude_dir: Option<Vec<String>>,
    /// Which ignore rules and dot entries the walk leaves out.
    pub ignore: crate::search::walk::IgnorePolicy,
    /// Maximum path depth below a directory root (1 = direct files; 0 = no descent). `None` = unbounded.
    pub max_depth: Option<u32>,
    pub max_files: Option<u32>,
    /// Leading candidates (walk order) left unevaluated: an earlier scan of
    /// the same scope already evaluated them. They still count toward
    /// `max_files`.
    pub skip_files: Option<u32>,
    pub max_file_bytes: Option<u32>,
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
    pub diagnostics: Vec<StructuralDiagnostic>,
}

pub struct StructuralSearchFilesDetailedResult {
    pub scan_truncated: bool,
    /// Where a truncated scan resumes: the walk-order index of the first
    /// unevaluated candidate (pass it back as `skip_files`). `None` when the
    /// scan is complete, or was cut by an explicit `max_files`.
    pub scan_resume: Option<u32>,
    /// The first candidate (walk order) the walk cap left out: the walk
    /// stopped before it, so it and every later candidate are unread. `None`
    /// when the walk reached the end of its scope.
    pub scan_frontier: Option<PathBuf>,
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
    pub status: String,
    pub query: StructuralQueryExplanation,
    pub diagnostics: Vec<StructuralDiagnostic>,
    pub warnings: Vec<String>,
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
        self.skipped = self.skipped.saturating_add(1);
        if error
            .io_error()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
        {
            self.permission_denied = self.permission_denied.saturating_add(1);
        }
        if self.sample.len() < WALK_ERROR_SAMPLE {
            self.sample.push(error.to_string());
        }
    }
}

impl StructuralDiagnostic {
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
