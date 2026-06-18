/* Public types for @octocodeai/octocode-context-utils */
/**
 * Agent-readable "standard" view: strips comments and blank-line noise while
 * preserving indentation and code shape. Capped at 1MB; panic-contained.
 */
export declare function applyContentViewMinification(content: string, filePath: string): string

/**
 * Full minification that never grows the content — returns the minified
 * form only when it is shorter, otherwise the original. Panic-contained.
 */
export declare function applyMinification(content: string, filePath: string): string

/**
 * Structural skeleton with an `NNN| ` line-number gutter: tree-sitter for
 * configured parser-backed languages, document outlines for Markdown,
 * heuristics for the rest. Returns `null` for data, config, unsupported
 * prose formats, and content above the 1MB guard.
 */
export declare function extractSignatures(content: string, filePath: string): string | null

/**
 * One structural (AST) search match. Line numbers are 1-based so `startLine`
 * can be passed straight to `lspGetSemantics` as a `lineHint`; columns are
 * 0-based character offsets (tree-sitter native).
 */
export interface StructuralMatch {
  startLine: number
  endLine: number
  startCol: number
  endCol: number
  text: string
  /**
   * Captured metavariables, keyed by the bare name (no leading `$`). A `$X`
   * capture yields a single-element array; a `$$$ARGS` capture yields the full
   * list of captured nodes (separators such as `,` are included, matching
   * ast-grep exactly).
   */
  metavars: Record<string, Array<string>>
}

/**
 * Structural (AST) search — octocode's L2 layer between text (ripgrep) and
 * semantics (LSP). The grammar is resolved from `filePath`'s extension and the
 * source is matched against EITHER an ast-grep `pattern` (e.g. `eval($X)`) OR a
 * YAML `rule` blob (relational/composite: `not`/`inside`/`has`/`all`/`any`) —
 * pass exactly one. Returns node ranges plus captured metavariables. Throws on
 * an unsupported extension, an invalid pattern/rule, or both/neither query.
 */
export declare function structuralSearch(
  content: string,
  filePath: string,
  pattern?: string | undefined | null,
  rule?: string | undefined | null
): Array<StructuralMatch>

export type CommentPatternGroup =
  | 'c-style'
  | 'hash'
  | 'html'
  | 'sql'
  | 'lua'
  | 'haskell'
  | 'semicolon'
  | 'wasm-text'
  | 'percent'
  | 'haml'
  | 'slim'
  | 'powershell'
  | 'bang'
  | 'apostrophe'
  | 'double-dash'
  | 'fsharp-block'
  | 'pascal'
  | 'template'
  | 'python-docstring'

export type MinifyStrategy = 'aggressive' | 'conservative' | 'general' | 'json' | 'terser'

export interface FileTypeMinifyConfig {
  strategy: MinifyStrategy | string
  comments?: CommentPatternGroup | CommentPatternGroup[] | null
}

export interface MinifyConfigSnapshot {
  fileTypes: Record<
    string,
    {
      strategy: MinifyStrategy | string
      comments: CommentPatternGroup | CommentPatternGroup[] | null
    }
  >
}

export type JsonPrimitive = string | number | boolean | null
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue }
export type JsonInput =
  | JsonPrimitive
  | undefined
  | JsonInput[]
  | { [key: string]: JsonInput }

/**
 * Extract the file extension from a path (dotfile-aware).
 * Options control lowercasing and the configured default used when no extension exists.
 */
export declare function getExtension(filePath: string, options?: GetExtensionOptions | undefined | null): string

export interface GetExtensionOptions {
  lowercase?: boolean
  fallback?: string
}

/**
 * Returns the full MINIFY_CONFIG as a JS-compatible object.
 * Shape: `{ fileTypes: Record<string, { strategy: string, comments: string | string[] | null }> }`
 */
export declare function getMINIFY_CONFIG(): MinifyConfigSnapshot

/**
 * Returns a sorted list of JS char offsets (UTF-16 code units) where
 * top-level semantic blocks begin in `content`.
 *
 * **Tree-sitter** (exact AST): `ts tsx js jsx mjs cjs py go rs java c h sh bash zsh`
 * **Heuristic** (pattern-based): `cpp hpp cc cxx cs kt kotlin scala rb php swift
 *   css scss less html htm sql vue svelte ex exs hs lhs md lua` + 10 more
 * **Returns `[]`** for data/config files (`json yaml toml ini csv xml …`),
 *   plain text, and files above the 1 MB guard.
 *
 * Char offsets match JavaScript `string.substring()` — pass them directly to
 * the TypeScript pagination layer without conversion.
 */
export declare function getSemanticBoundaryOffsets(content: string, filePath: string): Array<number>

/**
 * Returns all extensions that have signature extraction support
 * (tree-sitter languages + heuristic-covered languages).
 */
export declare function getSupportedSignatureExtensions(): Array<string>

/**
 * Serialize a JSON value to YAML — the formatter for every MCP tool
 * response. Optional key sorting and priority-key ordering; multiline
 * strings become block scalars. Emission is locked by yaml_utils tests.
 */
export declare function jsonToYamlString(jsonObject: JsonInput, config?: YamlConversionConfig | undefined | null): string

/**
 * Aggressive strategy: strip comments, collapse all whitespace, tighten
 * punctuation. Lossy — for token-budget views only.
 */
export declare function minifyAggressiveCore(content: string, config: FileTypeMinifyConfig): string

/**
 * Whitespace-only code cleanup: trim line ends, collapse 3+ blank lines,
 * preserve indentation.
 */
export declare function minifyCodeCore(content: string): string

/**
 * Conservative strategy: strip the configured comment groups, collapse blank
 * runs, preserve indentation.
 */
export declare function minifyConservativeCore(content: string, config: FileTypeMinifyConfig): string

/**
 * Full minification on libuv's worker pool.
 * Returns a Promise from JavaScript and does not block the event loop.
 */
export declare function minifyContent(content: string, filePath: string): Promise<MinifyResult>

/** Sync equivalent of TS `minifyContent` — returns MinifyResult. */
export declare function minifyContentResult(content: string, filePath: string): MinifyResult

/**
 * Full minification, synchronous. Content above the 1MB guard is returned
 * unchanged; unknown file types use the general strategy.
 */
export declare function minifyContentSync(content: string, filePath: string): string

/**
 * Lightweight CSS cleanup (comment strip + whitespace). See
 * `minifyCSSQuality` for the lightningcss-backed variant.
 */
export declare function minifyCSSCore(content: string): string

/**
 * CSS minification via lightningcss — parser-grade, strips comments and
 * redundant units.
 */
export declare function minifyCSSQuality(content: string): string

/** Generic text cleanup for unknown file types: trim + collapse blank runs. */
export declare function minifyGeneralCore(content: string): string

/**
 * Lightweight HTML/XML cleanup (comment strip + whitespace). See
 * `minifyHTMLQuality` for the minify-html-backed variant.
 */
export declare function minifyHTMLCore(content: string): string

/**
 * HTML minification via minify-html — parser-grade comment and whitespace
 * removal.
 */
export declare function minifyHTMLQuality(content: string): string

/**
 * Heuristic JS minifier (comment strip + whitespace tightening) used when
 * the OXC pipeline declines the input.
 */
export declare function minifyJavaScriptCore(content: string): string

/**
 * Compact JSON to a single line. JSONC/JSON5 noise (comments, trailing
 * commas) is stripped before parsing; unparseable input is returned trimmed.
 */
export declare function minifyJsonCore(content: string): MinifyResult

/**
 * Readable JSON view: keeps formatting, strips JSONC noise and trailing
 * whitespace, collapses blank runs. Valid JSON passes through unchanged.
 */
export declare function minifyJsonReadable(content: string): MinifyResult

/**
 * Markdown view: drops HTML comments, badges, and generated TOCs; compacts
 * tables and headings; preserves code fences and frontmatter verbatim.
 */
export declare function minifyMarkdownCore(content: string): string

export interface MinifyResult {
  content: string
  failed: boolean
  /** Strategy name or "failed" */
  type: string
  reason?: string
}

/** `commentTypes` accepts a single string or array of strings. */
export declare function removeComments(content: string, commentTypes: CommentPatternGroup | CommentPatternGroup[]): string

export const SIGNATURES_ONLY_HINT: string

/**
 * Remove Python docstrings (module/class/function level) while preserving
 * all runtime code.
 */
export declare function stripPythonDocstrings(content: string): string

export interface YamlConversionConfig {
  sortKeys?: boolean
  keysPriority?: Array<string>
}

export declare const MINIFY_CONFIG: {
  fileTypes: Record<string, { strategy: MinifyStrategy | string; comments: CommentPatternGroup | CommentPatternGroup[] | null }>
}
export declare const SUPPORTED_SIGNATURE_EXTENSIONS: readonly string[]

// ── Ripgrep NDJSON parser ─────────────────────────────────────────────────────

export interface RipgrepParseOptions {
  /** Number of context lines around each match (default 0). */
  contextLines?: number
  /** Max Unicode chars per match snippet (default 500). */
  maxSnippetChars?: number
}
export interface RipgrepMatch {
  /** 1-based line number. */
  line: number
  /** 0-based column offset of the first submatch. */
  column: number
  /** Assembled match + context window, truncated to `maxSnippetChars`. */
  value: string
}
export interface RipgrepFile {
  path: string
  matchCount: number
  matches: RipgrepMatch[]
}
export interface RipgrepStats {
  matchCount?: number
  matchedLines?: number
  filesMatched?: number
  filesSearched?: number
  bytesSearched?: number
  searchTime?: string
}
export interface RipgrepParseResult {
  files: RipgrepFile[]
  stats: RipgrepStats
}

/**
 * Parse ripgrep `--json` NDJSON stdout into structured files + stats.
 * Replaces the TypeScript `parseRipgrepJson` (single `serde_json` streaming pass,
 * no Zod, no per-line JSON.parse, no `[...value]` spread).
 */
export declare function parseRipgrepJson(stdout: string, options?: RipgrepParseOptions | undefined | null): RipgrepParseResult

// ── Filesystem query ──────────────────────────────────────────────────────────

export interface FileSystemQueryOptions {
  path: string
  /** Include the root path itself in results (default false). */
  includeRoot?: boolean
  /** Descend into child directories (default true). */
  recursive?: boolean
  /** Maximum depth where direct children are depth 1. */
  maxDepth?: number
  /** Minimum depth where direct children are depth 1. */
  minDepth?: number
  /** Include dotfiles and dot-directories (default true). */
  showHidden?: boolean
  /** Match basename globs, OR-combined. */
  names?: string[]
  /** Match full path glob. */
  pathPattern?: string
  /** Rust regex against basename. */
  regex?: string
  /** POSIX find-style entry type: f=file, d=directory, l=symlink. */
  entryType?: 'f' | 'd' | 'l' | string
  /** Match only empty files or directories. */
  empty?: boolean
  /** Modified within a duration string such as 7d, 2h, 30m. */
  modifiedWithin?: string
  /** Modified before a duration string such as 30d. */
  modifiedBefore?: string
  /** Accessed within a duration string such as 7d. */
  accessedWithin?: string
  /** Size greater than a string such as 100k, 1m, 500b. */
  sizeGreater?: string
  /** Size less than a string such as 100k, 1m, 500b. */
  sizeLess?: string
  /** Exact octal permissions, e.g. 644. */
  permissions?: string
  executable?: boolean
  readable?: boolean
  writable?: boolean
  excludeDir?: string[]
  /** Store at most this many matching entries while still counting matches. */
  limit?: number
}

export interface FileSystemEntry {
  /** Absolute or input-root-relative path as returned by the platform. */
  path: string
  /** Path relative to the query root. */
  relativePath: string
  name: string
  /** "file", "directory", "symlink", or "other". */
  entryType: string
  size?: number
  modifiedMs?: number
  accessedMs?: number
  permissions?: string
  extension?: string
  /** Output depth where direct children are 0. */
  depth: number
}

export interface FileSystemQueryResult {
  entries: FileSystemEntry[]
  totalDiscovered: number
  wasCapped: boolean
  skipped: number
  permissionDenied: number
  warnings: string[]
}

/**
 * Cross-platform filesystem traversal and metadata filtering for local tools.
 * TypeScript callers keep MCP response shaping and hints.
 */
export declare function queryFileSystem(options: FileSystemQueryOptions): FileSystemQueryResult

// ── UTF-8 offset helpers ──────────────────────────────────────────────────────

/**
 * Number of UTF-8 bytes up to (not including) the `charIndex`-th JavaScript
 * UTF-16 code unit in `content`. Zero-allocation — replaces `Buffer.byteLength(content.substring(0, charIndex))`.
 */
export declare function charToByteOffset(content: string, charIndex: number): number

/**
 * JavaScript UTF-16 code-unit offset for `byteOffset` bytes into `content`.
 * Zero-allocation — replaces `Buffer.from(content, 'utf8').slice(0, offset).toString('utf8').length`.
 */
export declare function byteToCharOffset(content: string, byteOffset: number): number

/**
 * Extract a byte-range substring from `content`.
 * Replaces `Buffer.from(content, 'utf8').slice(start, end).toString('utf8')`.
 */
export declare function byteSliceContent(content: string, byteStart: number, byteEnd: number): string

export interface SliceContentOptions {
  /** Snap start to line start and end to line end (default false). */
  snapToLineBoundary?: boolean
}
export interface SliceContentResult {
  text: string
  /** Actual start char offset (may differ from requested when snapping). */
  charOffset: number
  charLength: number
  byteOffset: number
  byteLength: number
  hasMore: boolean
  nextCharOffset?: number
}

/**
 * Paginate `content` by char offset + length, with optional line-boundary snapping.
 * Replaces the char-mode block in `applyPagination` and the dead-code
 * `sliceByCharRespectLines` (0 callers confirmed by LSP).
 */
export declare function sliceContent(content: string, charOffset: number, charLength: number, options?: SliceContentOptions | undefined | null): SliceContentResult

// ── In-memory line extractor ──────────────────────────────────────────────────

export interface ExtractMatchingLinesOptions {
  /** Treat `pattern` as a regex (default false — literal match). */
  isRegex?: boolean
  /** Case-sensitive match (default false). */
  caseSensitive?: boolean
  /** Lines of context to include around each match (default 0). */
  contextLines?: number
  /** Cap the number of matched lines returned. */
  maxMatches?: number
}
export interface MatchRange {
  /** 1-based inclusive start line. */
  start: number
  /** 1-based inclusive end line. */
  end: number
}
export interface ExtractMatchingLinesResult {
  /** Output lines including context and omission markers. */
  lines: string[]
  /** 1-based line numbers of actual matches (capped by `maxMatches`). */
  matchingLines: number[]
  /** Total matches before `maxMatches` cap. */
  matchCount: number
  matchRanges: MatchRange[]
}

/**
 * Search `content` line-by-line for `pattern` (literal or regex), returning
 * matched lines with context windows and omission markers.
 * Replaces `extractMatchingLines` (contentExtractor.ts) — single DFA-backed pass.
 */
export declare function extractMatchingLines(content: string, pattern: string, options?: ExtractMatchingLinesOptions | undefined | null): ExtractMatchingLinesResult

// ── Unified diff parser / filter ──────────────────────────────────────────────

export interface FilterPatchOptions {
  /** Only keep additions at these new-file line numbers. */
  additions?: number[]
  /** Only keep deletions at these original-file line numbers. */
  deletions?: number[]
  /** Apply context trimming (equivalent to `trimDiffContext`, default false). */
  trimContext?: boolean
  /** Context window size when `trimContext` is true (default 2). */
  contextLines?: number
}

/**
 * Filter and optionally trim a unified diff patch in a single pass.
 * Replaces `filterPatch` + `trimDiffContext` from `utils/parsers/diff.ts`.
 */
export declare function filterPatch(patch: string, options?: FilterPatchOptions | undefined | null): string
