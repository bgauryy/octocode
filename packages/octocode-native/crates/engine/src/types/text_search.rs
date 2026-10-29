//! Text search options, results and their enums.

#[derive(Debug, Clone)]
pub struct TextSearchMatch {
    /// 1-based line number.
    pub line: u32,
    /// The match's last line when it spans several (multiline mode); `None`
    /// for a one-line match.
    pub last_line: Option<u32>,
    /// 0-based column offset of the first submatch.
    pub column: u32,
    /// 0-based byte offset of the first submatch within its line (UTF-8): where
    /// a byte-window read of a long line starts at the match.
    pub byte_offset: u32,
    /// Assembled match + context window, truncated to `max_snippet_chars`.
    pub value: String,
    /// Reconstruct this ordinary, context-free line from its source before
    /// displaying it. `Some(0)` is unpriced; a positive value is the runtime's
    /// exact redacted JSON row cost. Empty `value` is not the source text.
    /// Nonempty values are retained redactions or a materialized page.
    pub deferred: Option<u32>,
    /// SHA-256 of the trimmed value before clipping. Duplicate ranking uses
    /// this identity so changing the display width cannot reorder rows.
    pub value_digest: [u8; 32],
    /// Frequency for this value when `count_unique` is enabled.
    pub count: Option<u32>,
    /// Lexical hit rank of the matched line (see `relevance::line_rank`):
    /// 3 declared name, 2 deciding statement (assignment, branch, return,
    /// raise), 1 other code, 0 comment or string. Set for line matches;
    /// `None` for only-matching spans.
    pub rank: Option<u32>,
    /// When the assembled content-view snippet was clipped to `max_snippet_chars`,
    /// the original (pre-truncation) Unicode-scalar length of the value. `None`
    /// when the snippet was not truncated. Lets callers surface a truncation
    /// indicator on content-view snippets, not just only-matching spans.
    pub original_chars: Option<u32>,
    /// The same pre-truncation value's length in UTF-8 bytes, set with
    /// `original_chars`: what a byte-paged read of the clipped line costs.
    pub original_bytes: Option<u32>,
}

impl TextSearchMatch {
    /// The identity digest of a match value (see `value_digest`): SHA-256 of
    /// the value trimmed of surrounding whitespace. Every producer of a row
    /// and every verifier that restates one uses this rule.
    pub fn digest_of(value: &str) -> [u8; 32] {
        use sha2::{Digest as _, Sha256};
        Sha256::digest(value.trim().as_bytes()).into()
    }
}

#[derive(Debug, Clone)]
pub struct TextSearchFile {
    pub path: String,
    pub match_count: u32,
    pub matches: Vec<TextSearchMatch>,
    /// The bytes this file's values came from, when the search was asked to
    /// prove them ([`SourceValues::Hashed`]).
    pub source: Option<SearchedSource>,
}

/// A searched file's identity, taken from the one read the search made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchedSource {
    /// Length of the opened file, equal to the bytes hashed.
    pub size: u64,
    /// Modification time of the opened file.
    pub modified: Option<std::time::SystemTime>,
    /// SHA-256 of every byte of the file, as the search read it.
    pub digest: [u8; 32],
}

#[derive(Debug, Clone, Default)]
pub struct TextSearchStats {
    pub match_count: Option<u32>,
    pub matched_lines: Option<u32>,
    pub files_matched: Option<u32>,
    pub files_searched: Option<u32>,
    pub bytes_searched: Option<i64>,
    pub capped: Option<bool>,
    /// The limits that ended the search early, in [`CapReason`] order; empty
    /// when it covered its scope.
    pub cap_reasons: Vec<CapReason>,
    /// Traversal or per-file search failures; nonzero means incomplete coverage.
    pub error_count: Option<u32>,
    /// Bounded first failure detail. Counts include every observed failure.
    pub first_error: Option<String>,
    /// The share of `error_count` where a readable file failed in the regex
    /// matcher (a PCRE2 match or JIT-stack limit), not in reading it.
    pub pattern_error_count: Option<u32>,
    /// Files searched only up to their first NUL byte (`binaryQuit`) after
    /// real text, every one in path order; `binary_file_count` counts them.
    pub binary_files: Option<Vec<String>>,
    pub binary_file_count: Option<u32>,
    /// Matched files with a returned line that is not UTF-8, in path order:
    /// their values are decoded text (Latin-1, or U+FFFD for invalid bytes,
    /// as [`crate::text::Decoded::interior`] reads them), not verbatim bytes.
    pub not_utf8_files: Option<Vec<String>>,
    /// Files binary from their leading bytes (a NUL before any text, or
    /// after a short header that matched nothing), skipped as opaque
    /// binary. Nothing text-searchable was lost, so they are not a coverage gap.
    pub skipped_binary_count: Option<u32>,
    /// `skipped_binary_count` per lowercased extension ("" for none), most
    /// files first.
    pub skipped_binary_extensions: Option<Vec<BinaryExtensionCount>>,
    /// Root-relative paths of the directories `exclude_dir` pruned, sorted:
    /// what the search did not cover.
    pub pruned_dirs: Option<Vec<String>>,
    /// Files with a match that `file_contains_all` / `file_contains_none`
    /// removed: why a filtered search can come back empty.
    pub file_terms_dropped: Option<u32>,
    /// Non-empty directories at `max_depth` the walk did not enter: the
    /// files below them were not searched.
    pub depth_cut_dirs: Option<u32>,
}

/// A limit that ended a text search before it covered its scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapReason {
    /// More files matched than the collection cap keeps.
    MaxCollectedFiles,
    /// A file held more only-matching spans than are kept per file.
    MaxOnlyMatchingPerFile,
    /// The PCRE2 wall-clock deadline stopped the search.
    Pcre2Deadline,
    /// A file over the byte ceiling was skipped.
    MaxFileSize,
    /// The caller stopped the search.
    Cancelled,
    /// A file was searched only up to its first NUL byte.
    BinaryQuit,
}

impl CapReason {
    /// The reason's wire name (`capReason`, `partialReasons`).
    pub fn name(self) -> &'static str {
        match self {
            Self::MaxCollectedFiles => "maxCollectedFiles",
            Self::MaxOnlyMatchingPerFile => "maxOnlyMatchingPerFile",
            Self::Pcre2Deadline => "pcre2Deadline",
            Self::MaxFileSize => "maxFileSize",
            Self::Cancelled => "cancelled",
            Self::BinaryQuit => "binaryQuit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExtensionCount {
    pub extension: String,
    pub count: u32,
    /// The extensionless group (`extension` "") names its files, sorted:
    /// no extension filter can list them without listing text files too.
    pub names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TextSearchResult {
    pub files: Vec<TextSearchFile>,
    pub stats: TextSearchStats,
}

/// Resolved context around a search hit. Tool defaults and overrides are
/// resolved by the caller; the engine and page layout share these bounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchContext {
    pub before: u32,
    pub after: u32,
}

impl SearchContext {
    pub const fn symmetric(lines: u32) -> Self {
        Self {
            before: lines,
            after: lines,
        }
    }

    pub fn is_empty(self) -> bool {
        self.before == 0 && self.after == 0
    }
}

/// Options for Octocode's in-process text search. Each field is one search
/// behaviour; the CLI-style flags in field docs (`-C`, `-g`, `-t`) name the
/// familiar equivalent.
#[derive(Debug, Clone, Default)]
pub struct TextSearchOptions {
    /// Search root: a directory (recursive) or a single file.
    pub path: String,
    /// The search pattern (`matchString`).
    pub pattern: String,

    // ── match flags ──────────────────────────────────────────────────────────
    /// How the pattern reads: literal text (`-F`), a Rust regex (default) or
    /// a PCRE2 regex for lookaround/backreferences (`-P`).
    pub syntax: PatternSyntax,
    /// Smart-case (`-S`, default), case-sensitive (`-s`) or
    /// case-insensitive (`-i`).
    pub case: CaseRule,
    /// Match whole words only (`-w`).
    pub whole_word: Option<bool>,
    /// Invert: report non-matching lines (`-v`).
    pub invert_match: Option<bool>,
    /// Multi-line mode: `.` and the pattern may span lines (`-U`).
    pub multiline: Option<bool>,
    /// In multi-line mode, let `.` match newlines (`--multiline-dotall`).
    pub multiline_dotall: Option<bool>,

    // ── output ───────────────────────────────────────────────────────────────
    /// What each matched file reports.
    pub output: SearchOutput,

    // ── filters ──────────────────────────────────────────────────────────────
    /// Context lines before and after each match (`-B`, `-A`).
    pub context: SearchContext,
    /// Restrict to a file type, e.g. `ts`, `py` (`-t`).
    pub lang_type: Option<String>,
    /// File-level AND: keep a matched file only when it also contains every
    /// term (smart-case each: all-lowercase matches any case).
    pub file_contains_all: Option<Vec<String>>,
    /// File-level NOT: drop a matched file that contains any term.
    pub file_contains_none: Option<Vec<String>>,
    /// Include globs (`-g <glob>`).
    pub include: Option<Vec<String>>,
    /// Exclude globs (`-g !<glob>`).
    pub exclude: Option<Vec<String>>,
    /// Exclude directories (`-g !<dir>/`).
    pub exclude_dir: Option<Vec<String>>,
    /// Which ignore rules and dot entries the walk leaves out.
    pub ignore: crate::search::walk::IgnorePolicy,
    /// Maximum directory descent below the search root. `0` searches files
    /// directly in the root, `1` includes one nested directory level, and so on.
    pub max_depth: Option<u32>,

    // ── ordering & result shaping ────────────────────────────────────────────
    /// The order of the collected files (and which survive the collection
    /// cap).
    pub sort: SearchSort,
    /// Reverse the sort order (`--sortr`).
    pub sort_reverse: Option<bool>,
    /// Max Unicode chars per assembled snippet (default 500).
    pub max_snippet_chars: Option<u32>,

    /// Native collection guard: stop after this many matched files have been
    /// collected. Distinct from native runtime maxFiles, which is a per-page UI
    /// size, not an engine resource cap. The cap is applied as a stable
    /// truncation of the fully sorted result set, so page 1 is deterministic and
    /// pagination snapshots stay valid across runs.
    pub max_collected_files: Option<u32>,
    /// Per-file byte ceiling: a file larger than this is skipped before it is
    /// searched (surfaced as a `maxFileSize` cap reason). `None` uses
    /// [`DEFAULT_MAX_SEARCH_FILE_BYTES`]. Guards against OOM on pathological
    /// multi-GB single-line files.
    pub max_file_bytes: Option<u32>,
    /// Worker threads for the parallel directory walk. `None` (or `0`) keeps
    /// the ignore crate's default of one per available core; a caller running
    /// several walks at once passes its share of the cores. Ignored when
    /// `sort` is `traversal`, which always walks on one thread.
    pub walk_threads: Option<u32>,
    /// How values are kept and whether each kept file's bytes are hashed.
    pub values: SourceValues,
}

impl TextSearchOptions {
    /// The same search over the same scope reporting `output`, without
    /// context, digests or a ranked order: a bounded side search (a probe)
    /// asks only which files or how many lines match.
    pub fn probe(&self, output: SearchOutput) -> Self {
        Self {
            output,
            context: SearchContext::default(),
            sort: SearchSort::Path,
            values: SourceValues::Owned,
            ..self.clone()
        }
    }
}

/// How a text search reads its pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PatternSyntax {
    /// Literal text (`-F`).
    Literal,
    /// A Rust regex: linear time, no lookaround.
    #[default]
    Regex,
    /// A PCRE2 regex (`-P`): lookaround and backreferences, bounded by a
    /// wall-clock deadline.
    Pcre2,
}

/// How a text search compares letter case.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaseRule {
    /// Case-insensitive unless the pattern holds an uppercase letter (`-S`).
    #[default]
    Smart,
    /// Case-sensitive (`-s`).
    Sensitive,
    /// Case-insensitive (`-i`).
    Insensitive,
}

/// What a text search reports for each matched file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchOutput {
    /// The matching lines, with context.
    #[default]
    Lines,
    /// One value per submatch, set to the matched span instead of the
    /// whole line (`-o`): on a minified one-liner, line mode can only count
    /// hits, this enumerates them.
    Spans(SpanValues),
    /// Only the paths of files that contain a match (`-l`).
    FilesWithMatch,
    /// Only the paths of files with no match (`--files-without-match`).
    FilesWithoutMatch,
    /// Per-file count of matching lines (`-c`).
    CountLines,
    /// Per-file count of individual matches (`--count-matches`).
    CountMatches,
}

/// Which spans a [`SearchOutput::Spans`] search keeps per file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpanValues {
    /// Every span.
    #[default]
    Every,
    /// Each distinct value once, in first-occurrence order and anchor.
    Unique,
    /// Each distinct value once with its frequency, most frequent first.
    Counted,
}

/// The order of a text search's collected files. Every key ends with the
/// path, so a collection cap always keeps the same files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchSort {
    /// Lexicographic by full path.
    #[default]
    Path,
    /// The walk's own order (one walk thread).
    Traversal,
    /// Declarations of a searched identifier, coverage, density and source
    /// paths first (see `search::relevance`).
    Relevance,
    /// Most matches first.
    MatchCount,
    /// Ascending modification time.
    Modified,
    /// Ascending access time.
    Accessed,
    /// Ascending creation time.
    Created,
}

/// How a text search keeps its values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SourceValues {
    /// Owned snippets; no file is hashed.
    #[default]
    Owned,
    /// Owned snippets, and each kept file is hashed by a second whole-file
    /// read right after its search and reported as
    /// [`TextSearchFile::source`], so a caller can prove the values later
    /// without reading the file again.
    Hashed,
    /// [`Self::Hashed`], keeping anchors and complete value digests instead
    /// of owned snippets for ordinary context-free, single-line searches;
    /// other views keep their owned values.
    Deferred,
}

impl SourceValues {
    /// Whether each kept file is hashed.
    pub fn hashed(self) -> bool {
        self != Self::Owned
    }
}
