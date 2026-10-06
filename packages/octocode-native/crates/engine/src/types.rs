/// One parser entry from the canonical grammar registry. Consumers use this
/// runtime inventory for language selection and agent guidance instead of
/// maintaining extension/name tables outside the engine.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarCapability {
    pub language: String,
    pub language_id: Option<String>,
    pub selector_aliases: Vec<String>,
    pub extensions: Vec<String>,
    pub structural_search: bool,
    pub signature_outline: bool,
    pub graph_facts: bool,
}

// ── ripgrep_parser types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RipgrepMatch {
    /// 1-based line number.
    pub line: u32,
    /// 0-based column offset of the first submatch.
    pub column: u32,
    /// Assembled match + context window, truncated to `max_snippet_chars`.
    pub value: String,
    /// Frequency for this value when `count_unique` is enabled.
    pub count: Option<u32>,
    /// AST node-kind label (declaration|import|export|callsite|identifier|
    /// comment|string|configKey|heading) when `classify_matches` is enabled.
    /// `None` when classification was off, the language is unsupported, or the
    /// file failed to parse.
    pub kind: Option<String>,
    /// Deterministic relevance hint (0.0..1.0) derived from `kind`.
    pub score_hint: Option<f64>,
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
}

#[derive(Debug, Clone)]
pub struct RipgrepFile {
    pub path: String,
    pub match_count: u32,
    pub matches: Vec<RipgrepMatch>,
}

#[derive(Debug, Clone, Default)]
pub struct RipgrepStats {
    pub match_count: Option<u32>,
    pub matched_lines: Option<u32>,
    pub files_matched: Option<u32>,
    pub files_searched: Option<u32>,
    pub bytes_searched: Option<i64>,
    pub search_time: Option<String>,
    pub capped: Option<bool>,
    pub cap_reason: Option<String>,
    /// Traversal or per-file search failures; nonzero means incomplete coverage.
    pub error_count: Option<u32>,
    /// Bounded first failure detail. Counts include every observed failure.
    pub first_error: Option<String>,
    /// Files searched only up to their first NUL byte (`binaryQuit`) after
    /// real text, every one in path order; `binary_file_count` counts them.
    pub binary_files: Option<Vec<String>>,
    pub binary_file_count: Option<u32>,
    /// Files binary from their leading bytes (a NUL before any text, or
    /// after a short header that matched nothing), skipped like rg skips
    /// them. Nothing text-searchable was lost, so they are not a coverage gap.
    pub skipped_binary_count: Option<u32>,
    /// `skipped_binary_count` per lowercased extension ("" for none), most
    /// files first.
    pub skipped_binary_extensions: Option<Vec<BinaryExtensionCount>>,
    /// Root-relative paths of the directories `exclude_dir` pruned, sorted:
    /// what the search did not cover.
    pub pruned_dirs: Option<Vec<String>>,
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
pub struct RipgrepParseResult {
    pub files: Vec<RipgrepFile>,
    pub stats: RipgrepStats,
}

/// Options for the in-process ripgrep search (`searchRipgrep`). Field semantics
/// mirror the ripgrep CLI flags the old `RipgrepCommandBuilder` emitted, so the
/// search behaves identically to shelling out to `rg`.
#[derive(Debug, Clone, Default)]
pub struct RipgrepSearchOptions {
    /// Search root: a directory (recursive) or a single file.
    pub path: String,
    /// The search pattern (rg's positional pattern / `keywords`).
    pub pattern: String,

    // ── match flags ──────────────────────────────────────────────────────────
    /// Treat the pattern as a literal string, not a regex (`-F`).
    pub fixed_string: Option<bool>,
    /// Use the PCRE2 engine for lookaround/backreferences (`-P`).
    pub perl_regex: Option<bool>,
    /// Case-sensitive match (`-s`). Wins over `case_insensitive`.
    pub case_sensitive: Option<bool>,
    /// Case-insensitive match (`-i`). Default is smart-case (`-S`).
    pub case_insensitive: Option<bool>,
    /// Match whole words only (`-w`).
    pub whole_word: Option<bool>,
    /// Invert: report non-matching lines (`-v`).
    pub invert_match: Option<bool>,
    /// Multi-line mode: `.` and the pattern may span lines (`-U`).
    pub multiline: Option<bool>,
    /// In multi-line mode, let `.` match newlines (`--multiline-dotall`).
    pub multiline_dotall: Option<bool>,

    // ── output modes (mutually exclusive; first set wins, matching the CLI) ───
    /// List only the paths of files that contain a match (`-l`).
    pub files_only: Option<bool>,
    /// List only the paths of files with no match (`--files-without-match`).
    pub files_without_match: Option<bool>,
    /// Per-file count of matching lines (`-c`).
    pub count_lines_per_file: Option<bool>,
    /// Per-file count of individual matches (`--count-matches`).
    pub count_matches_per_file: Option<bool>,

    // ── filters ──────────────────────────────────────────────────────────────
    /// Context lines around each match (`-C`).
    pub context_lines: Option<u32>,
    /// Restrict to a ripgrep file type, e.g. `ts`, `py` (`-t`).
    pub lang_type: Option<String>,
    /// Include globs (`-g <glob>`).
    pub include: Option<Vec<String>>,
    /// Exclude globs (`-g !<glob>`).
    pub exclude: Option<Vec<String>>,
    /// Exclude directories (`-g !<dir>/`).
    pub exclude_dir: Option<Vec<String>>,
    /// Do not honor .gitignore/.ignore/etc. (`--no-ignore`).
    pub no_ignore: Option<bool>,
    /// Search hidden files and directories (`--hidden`).
    pub hidden: Option<bool>,
    /// Maximum directory descent below the search root. `0` searches files
    /// directly in the root, `1` includes one nested directory level, and so on.
    pub max_depth: Option<u32>,

    // ── ordering & result shaping ────────────────────────────────────────────
    /// Sort key: `path` (default), `modified`, `accessed`, or `created`.
    pub sort: Option<String>,
    /// Reverse the sort order (`--sortr`).
    pub sort_reverse: Option<bool>,
    /// Max Unicode chars per assembled snippet (default 500).
    pub max_snippet_chars: Option<u32>,
    /// When true, label each match with its AST node kind (tree-sitter) for
    /// language-aware ranking. Optional and capped by the caller; degrades to
    /// unlabeled matches on unsupported/unparseable files.
    pub classify_matches: Option<bool>,

    // ── only-matching (rg -o) ──────────────────────────────────────────────
    /// Emit one match per *submatch* with `value` set to the matched span
    /// (not the whole line) — ripgrep's `-o`/`--only-matching`. The win on a
    /// minified one-liner: line mode can only count hits, this enumerates them.
    pub only_matching: Option<bool>,
    /// With `only_matching`, collapse duplicate values per file, preserving
    /// first-occurrence order and anchor.
    pub unique: Option<bool>,
    /// With `only_matching`, collapse duplicate values and attach their
    /// frequency, sorted by count descending.
    pub count_unique: Option<bool>,
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
}

// ── filesystem query types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct FileSystemQueryOptions {
    pub path: String,
    /// Include the root path itself in results (default false).
    pub include_root: Option<bool>,
    /// Descend into child directories (default true).
    pub recursive: Option<bool>,
    /// Maximum depth where direct children are depth 1.
    pub max_depth: Option<u32>,
    /// Minimum depth where direct children are depth 1.
    pub min_depth: Option<u32>,
    /// Include dotfiles and dot-directories (default true).
    pub show_hidden: Option<bool>,
    /// Match basename globs, OR-combined.
    pub names: Option<Vec<String>>,
    /// Match file extensions, OR-combined. Values may include a leading dot.
    /// Directories are preserved so recursive structure views can keep context.
    pub extensions: Option<Vec<String>>,
    /// Match full path glob.
    pub path_pattern: Option<String>,
    /// Rust regex against basename.
    pub regex: Option<String>,
    /// POSIX find-style entry type: f=file, d=directory, l=symlink.
    pub entry_type: Option<String>,
    /// Match only empty files or directories.
    pub empty: Option<bool>,
    /// Modified within a duration string such as 7d, 2h, 30m.
    pub modified_within: Option<String>,
    /// Modified before a duration string such as 30d.
    pub modified_before: Option<String>,
    /// Accessed within a duration string such as 7d.
    pub accessed_within: Option<String>,
    /// Size greater than a string such as 100k, 1m, 500b.
    pub size_greater: Option<String>,
    /// Size less than a string such as 100k, 1m, 500b.
    pub size_less: Option<String>,
    /// Exact octal permissions, e.g. 644.
    pub permissions: Option<String>,
    pub executable: Option<bool>,
    pub readable: Option<bool>,
    pub writable: Option<bool>,
    /// Directory names pruned from recursive traversal. The engine has no
    /// default list: omission prunes nothing. The native runtime's prune
    /// policy (`policy::prune`) supplies the names for every tool walk.
    /// Pruned directories are reported in `pruned_dirs`.
    pub exclude_dir: Option<Vec<String>>,
    /// The caller's path globs to skip: without `/` a glob matches entry
    /// names at any depth, with `/` the path below the root. A matching
    /// directory is skipped with everything under it.
    pub exclude: Option<Vec<String>>,
    /// Stop walking after `limit` returned entries. Default true for interactive
    /// tools; set false when exact total_discovered is more important than
    /// latency.
    pub stop_at_limit: Option<bool>,
    /// Store at most this many matching entries while still counting matches
    /// when stop_at_limit is false.
    pub limit: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct FileSystemEntry {
    /// Absolute or input-root-relative path as returned by the platform.
    pub path: String,
    /// Path relative to the query root.
    pub relative_path: String,
    pub name: String,
    /// "file", "directory", "symlink", or "other".
    pub entry_type: String,
    pub size: Option<i64>,
    pub modified_ms: Option<f64>,
    pub accessed_ms: Option<f64>,
    pub permissions: Option<String>,
    pub extension: Option<String>,
    /// Output depth where direct children are 0.
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct FileSystemQueryResult {
    pub entries: Vec<FileSystemEntry>,
    pub total_discovered: u32,
    pub was_capped: bool,
    pub skipped: u32,
    pub permission_denied: u32,
    pub warnings: Vec<String>,
    /// Root-relative paths of the directories `exclude_dir` pruned, in walk
    /// order: what a listing or an empty result did not cover.
    pub pruned_dirs: Vec<String>,
}

// ── graph scan types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct GraphFactsScanOptions {
    pub path: String,
    pub exclude_dir: Option<Vec<String>>,
    /// The caller's path globs to skip (see [`FileSystemQueryOptions::exclude`]).
    pub exclude: Option<Vec<String>>,
    pub max_files: Option<u32>,
    pub max_file_bytes: Option<u32>,
    pub language_globs: Option<Vec<GraphLanguageGlob>>,
}

#[derive(Debug, Clone)]
pub struct GraphLanguageGlob {
    pub language: String,
    pub glob: String,
}

#[derive(Debug, Clone)]
/// Syntax-aware value references to one declaration, excluding its own name,
/// export clauses and call-callee positions (those are call edges). Comments
/// and string literals never count.
pub struct GraphReferenceCount {
    pub declaration_id: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct GraphFactsScanDiagnostic {
    pub relative_path: String,
    pub code: String,
    pub message: String,
}

// ── diff_parser types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum PatchLineType {
    Addition,
    Deletion,
    Context,
}

#[derive(Debug, Clone, Default)]
pub struct FilterPatchOptions {
    /// Only keep additions at these new-file line numbers.
    pub additions: Option<Vec<i64>>,
    /// Only keep deletions at these original-file line numbers.
    pub deletions: Option<Vec<i64>>,
    /// Apply context trimming (equivalent to `trimDiffContext`, default false).
    pub trim_context: Option<bool>,
    /// Context window size when `trim_context` is true (default 2).
    pub context_lines: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct YamlConversionConfig {
    pub sort_keys: Option<bool>,
    pub keys_priority: Option<Vec<String>>,
}
