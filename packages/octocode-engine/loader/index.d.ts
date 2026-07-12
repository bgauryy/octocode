/* Public types for @octocodeai/octocode-engine */
/* eslint-disable */
export declare class NativeLspClient {
  constructor(config: JsLanguageServerConfig)
  start(): Promise<void>
  stop(): Promise<void>
  /** Readiness after post-`initialized` indexing: `progressIdle` = a `$/progress` cycle drained to idle; `settledFallback` = no progress seen, only the settle window elapsed; `timeout` = progress still active at the deadline. */
  waitForReady(timeoutMs?: number | undefined | null): Promise<'progressIdle' | 'settledFallback' | 'timeout'>
  hasCapability(capability: string): boolean
  /** Server-selected LSP `positionEncoding` (utf-16 unless the server is non-conformant); null if omitted/not started. */
  positionEncoding(): string | null
  getRecentStderr(): Array<string>
  openDocument(filePath: string, content: string): Promise<void>
  closeDocument(filePath: string): Promise<void>
  getDefinition(filePath: string, line: number, character: number): Promise<Array<JsCodeSnippet>>
  getReferences(filePath: string, line: number, character: number, includeDeclaration?: boolean | undefined | null): Promise<Array<JsCodeSnippet>>
  getHover(filePath: string, line: number, character: number): Promise<any>
  getTypeDefinition(filePath: string, line: number, character: number): Promise<Array<JsCodeSnippet>>
  getImplementation(filePath: string, line: number, character: number): Promise<Array<JsCodeSnippet>>
  getDocumentSymbols(filePath: string): Promise<any>
  prepareCallHierarchy(filePath: string, line: number, character: number): Promise<any>
  incomingCalls(item: any): Promise<any>
  outgoingCalls(item: any): Promise<any>
  /** Project-wide fuzzy symbol search — `workspace/symbol`. Returns `WorkspaceSymbol[] | SymbolInformation[]`. */
  workspaceSymbol(query: string): Promise<any>
  /** Prepare a type-hierarchy item at a position — `textDocument/prepareTypeHierarchy`. */
  prepareTypeHierarchy(filePath: string, line: number, character: number): Promise<any>
  /** Supertypes (base classes / implemented interfaces) — `typeHierarchy/supertypes`. */
  typeHierarchySupertypes(item: any): Promise<any>
  /** Subtypes (subclasses / implementors) — `typeHierarchy/subtypes`. */
  typeHierarchySubtypes(item: any): Promise<any>
  /** Pull diagnostics for a single file — `textDocument/diagnostic` (LSP 3.17+). */
  getDiagnostics(filePath: string): Promise<any>
}

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

/** Extract a byte-range substring from `content`. */
export declare function byteSliceContent(content: string, byteStart: number, byteEnd: number): string

/**
 * JavaScript UTF-16 code-unit offset for `byte_offset` bytes into `content`.
 * Zero-allocation — no `Buffer.from()` needed.
 */
export declare function byteToCharOffset(content: string, byteOffset: number): number

/**
 * Number of UTF-8 bytes up to (not including) the `char_index`-th JavaScript
 * UTF-16 code unit in `content`. Zero-allocation — no `Buffer.from()` needed.
 */
export declare function charToByteOffset(content: string, charIndex: number): number

/** Convert an LSP `SymbolKind` numeric code to a human-readable string tag. */
export declare function convertSymbolKind(kind?: number | undefined | null): string

/** Return the LSP language identifier for the file at `file_path`. */
export declare function detectLanguageId(filePath: string): string | null

/**
 * Search `content` line-by-line for `pattern` (literal or regex), returning
 * matched lines with context windows and omission markers.
 *
 * Replaces `extractMatchingLines` (contentExtractor.ts) which performed 2–3
 * full `forEach` scans with per-line `toLowerCase` + `RegExp.test`.
 */
export declare function extractMatchingLines(content: string, pattern: string, options?: ExtractMatchingLinesOptions | undefined | null): ExtractMatchingLinesResult

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

export interface ExtractMatchingLinesResult {
  /** Output lines including context and omission markers. */
  lines: Array<string>
  /** 1-based line numbers of actual matches (capped by `max_matches`). */
  matchingLines: Array<number>
  /** Total matches before `max_matches` cap. */
  matchCount: number
  matchRanges: Array<MatchRange>
}

/**
 * Structural skeleton with an `NNN| ` line-number gutter, produced purely by
 * tree-sitter parsing. Returns `null` for data/config formats, any language
 * without a wired grammar, content above the 1MB guard, and any skeleton that
 * would not be smaller than the source.
 */
export declare function extractSignatures(content: string, filePath: string): string | null

/**
 * Native JS/TS document symbols (server-free) as a JSON `DocumentSymbol[]`.
 *
 * Parses ECMAScript/TypeScript *syntax* with oxc and walks declarations into
 * the LSP `DocumentSymbol` shape (nested, numeric `SymbolKind`, 0-based UTF-16
 * ranges). **No type inference** — in-file scope/binding accuracy only; type-aware
 * outlines still require a language server. Only `ts/tsx/js/jsx/mjs/cjs/mts/cts`
 * are handled.
 *
 * Returns `null` for non-JS/TS files, oversized content, a hard parse failure
 * (caller should fall back to `extractSignatures`), or a file with no
 * extractable top-level symbols.
 */
export declare function extractJsSymbols(content: string, filePath: string): string | null

/**
 * Native in-file references (server-free) for the JS/TS symbol under
 * `(line, character)` (0-based, UTF-16), as a JSON `Range[]` covering the
 * declaration and every resolved in-file reference (declaration first).
 *
 * **Same-file only** — oxc resolves bindings within one module; cross-file
 * references require a language server. No type inference. Returns `null` for
 * non-JS/TS files, oversized content, a parse failure, or when the cursor is
 * not on a resolvable binding/reference.
 */
export declare function findInFileReferences(content: string, filePath: string, line: number, character: number): string | null

export interface GraphFactPosition {
  line: number
  character: number
}

export interface GraphFactRange {
  start: GraphFactPosition
  end: GraphFactPosition
}

export interface GraphFactDeclaration {
  id: string
  name: string
  kind: string
  line: number
  range: GraphFactRange
  selectionRange: GraphFactRange
  exported: boolean
  parent?: string
}

export interface GraphFactImport {
  id: string
  specifier: string
  line: number
  importKind: string
  localName?: string
  importedName?: string
}

export interface GraphFactExport {
  id: string
  name: string
  line: number
  exportKind: string
  localName?: string
  source?: string
}

export interface GraphFactCall {
  id: string
  caller: string
  callee: string
  line: number
  range: GraphFactRange
  kind: string
}

export interface GraphFactEdge {
  id: string
  from: string
  to: string
  relation: string
  source: string
  line: number
}

export interface GraphFacts {
  kind: 'graphFacts'
  source: 'native-ast'
  language: string
  file: string
  declarations: Array<GraphFactDeclaration>
  imports: Array<GraphFactImport>
  exports: Array<GraphFactExport>
  calls: Array<GraphFactCall>
  edges: Array<GraphFactEdge>
  diagnostics: Array<string>
}

export interface GraphFactCapability {
  extension: string
  language: string
  languageId?: string
  structuralSearch: boolean
  signatureOutline: boolean
  graphFacts: boolean
  factFamilies: Array<string>
}

/**
 * Native graph facts as a JSON `GraphFacts` object, or null when no graph-fact
 * extractor supports the file. JS/TS use the richer OXC lane; other supported
 * source languages use tree-sitter syntax inventory. Cross-file semantic
 * identity still needs LSP proof.
 */
export declare function extractGraphFacts(content: string, filePath: string): string | null

/**
 * Canonical list of file extensions (lowercase, no leading dot) handled by the
 * native oxc JS/TS path (`extractJsSymbols` / `findInFileReferences`). Gate
 * native dispatch on this list instead of hardcoding it.
 */
export declare function getSupportedJsTsExtensions(): Array<string>

/**
 * Canonical list of file extensions (lowercase, no leading dot) that can emit
 * native graph facts. JS/TS use OXC; other entries use tree-sitter syntax
 * inventory.
 */
export declare function getSupportedGraphFactExtensions(): Array<string>

/**
 * JSON `GraphFactCapability[]` describing graph-fact coverage by extension.
 */
export declare function getGraphFactCapabilities(): string

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
  names?: Array<string>
  /** Match file extensions, OR-combined. Values may include a leading dot. Directories are preserved. */
  extensions?: Array<string>
  /** Match full path glob. */
  pathPattern?: string
  /** Rust regex against basename. */
  regex?: string
  /** POSIX find-style entry type: f=file, d=directory, l=symlink. */
  entryType?: string
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
  excludeDir?: Array<string>
  /** Store at most this many matching entries while still counting matches. */
  limit?: number
}

export interface FileSystemQueryResult {
  entries: Array<FileSystemEntry>
  totalDiscovered: number
  wasCapped: boolean
  skipped: number
  permissionDenied: number
  warnings: Array<string>
}

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

export interface FileTypeMinifyConfig {
  strategy: string
  comments?: CommentPatternGroup | CommentPatternGroup[] | null
}

/**
 * Filter and optionally trim a unified diff patch.
 *
 * Replaces `filterPatch` + `trimDiffContext` from `utils/parsers/diff.ts` which
 * called `patch.split('
')` independently in both functions. This combines
 * both operations in a single pass.
 */
export declare function filterPatch(patch: string, options?: FilterPatchOptions | undefined | null): string

export interface FilterPatchOptions {
  /** Only keep additions at these new-file line numbers. */
  additions?: Array<number>
  /** Only keep deletions at these original-file line numbers. */
  deletions?: Array<number>
  /** Apply context trimming (equivalent to `trimDiffContext`, default false). */
  trimContext?: boolean
  /** Context window size when `trim_context` is true (default 2). */
  contextLines?: number
}

/**
 * Myers line diff (`oldText` → `newText`). Returns a full edit script of
 * `{ opType, line }` ops (`same` | `add` | `remove`). Prefer this over an
 * O(N·M) LCS for agent edit previews on mid/large files.
 */
export declare function computeLineDiff(oldText: string, newText: string): Array<LineDiffOp>

export interface LineDiffOp {
  /** `"same"` | `"add"` | `"remove"` */
  opType: string
  line: string
}

/** Convert a `file://` URI string back to an absolute filesystem path. */
export declare function fromUri(uri: string): string

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
 * Return the default language server configuration for `file_path` inside
 * `workspace_root`.
 */
export declare function getLanguageServerForFile(filePath: string, workspaceRoot: string): JsLanguageServerConfig | null

/**
 * Returns the full MINIFY_CONFIG as a JS-compatible object.
 * Shape: `{ fileTypes: Record<string, { strategy: string, comments: string | string[] | null }> }`
 */
export declare function getMINIFY_CONFIG(): MinifyConfigSnapshot

export interface MinifyConfigSnapshot {
  fileTypes: Record<string, { strategy: string; comments: CommentPatternGroup | CommentPatternGroup[] | null }>
}

export declare const MINIFY_CONFIG: MinifyConfigSnapshot
export declare const SUPPORTED_SIGNATURE_EXTENSIONS: readonly string[]
export declare const SUPPORTED_GRAPH_FACT_EXTENSIONS: readonly string[]
export declare const SUPPORTED_STRUCTURAL_EXTENSIONS: readonly string[]

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
 * JavaScript string slicing without conversion.
 *
 * Runs on libuv's worker pool (tree-sitter parse) — returns a Promise.
 */
export declare function getSemanticBoundaryOffsets(content: string, filePath: string): Promise<Array<number>>

/**
 * Returns all extensions that have signature extraction support
 * (tree-sitter languages + heuristic-covered languages).
 */
export declare function getSupportedSignatureExtensions(): Array<string>

export declare function getSupportedStructuralExtensions(): Array<string>

/** Check whether `command` is available on `PATH`. */
export declare function isCommandAvailable(command: string): boolean

export interface JsCodeSnippet {
  uri: string
  range: JsRange
  content: string
  symbolKind?: string
  displayRange?: any
}

export interface JsExactPosition {
  line: number
  character: number
}

export interface JsFuzzyPosition {
  symbolName: string
  lineHint?: number
  orderHint?: number
}

export interface JsLanguageServerConfig {
  command: string
  args?: Array<string>
  workspaceRoot: string
  languageId?: string
  initializationOptions?: any
  /** Extra environment variables to inject into the language server process. */
  env?: Record<string, string>
}

/**
 * Serialize a JSON value to YAML — the formatter for every MCP tool
 * response. Optional key sorting and priority-key ordering; multiline
 * strings become block scalars. Emission is locked by yaml_utils tests.
 */
export type JsonPrimitive = string | number | boolean | null
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue }
export type JsonInput =
  | JsonPrimitive
  | undefined
  | JsonInput[]
  | { [key: string]: JsonInput }

export declare function jsonToYamlString(jsonObject: JsonInput, config?: YamlConversionConfig | undefined | null): string

export interface JsRange {
  start: JsExactPosition
  end: JsExactPosition
}

export interface JsResolvedSymbol {
  position: JsExactPosition
  foundAtLine: number
  lineOffset: number
  lineContent: string
}

export interface MatchRange {
  /** 1-based inclusive start line. */
  start: number
  /** 1-based inclusive end line. */
  end: number
}

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

/** Synchronous full minification result. */
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
 * `minifyHTMLQuality` for the style-aware built-in variant.
 */
export declare function minifyHTMLCore(content: string): string

/**
 * Style-aware HTML cleanup: strips comments, tightens whitespace, and minifies
 * embedded `<style>` blocks through the existing CSS pipeline.
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


/**
 * Parse ripgrep `--json` NDJSON stdout into structured files + stats.
 *
 * Replaces the TypeScript `parseRipgrepJson` (utils/parsers/ripgrep.ts) which
 * used `JSON.parse` + Zod `safeParse` per NDJSON line and a `[...value]`
 * UTF-16 spread per match snippet. A single `serde_json` streaming pass with
 * no per-line schema validation.
 */
export declare function parseRipgrepJson(stdout: string, options?: RipgrepParseOptions | undefined | null): RipgrepParseResult

/**
 * Run ripgrep in-process: walk `path`, search every file with ripgrep's own
 * engine, and return the same `{ files, stats }` shape the `--json` parser
 * produced. Replaces shelling out to an `rg` binary (and the `@vscode/ripgrep`
 * bundle) — octocode is now its own source of ripgrep.
 *
 * Runs on the libuv thread pool so the filesystem walk never blocks the event
 * loop, mirroring the old async `spawn` of `rg`.
 */
export declare function searchRipgrep(options: RipgrepSearchOptions): Promise<RipgrepParseResult>

export declare const enum PatchLineType {
  Addition = 'Addition',
  Deletion = 'Deletion',
  Context = 'Context'
}

/**
 * Cross-platform filesystem traversal and metadata filtering for local tools.
 *
 * Replaces the POSIX `find`/`ls` execution paths in octocode-tools-core while
 * keeping MCP response shaping in TypeScript.
 */
export declare function queryFileSystem(options: FileSystemQueryOptions): FileSystemQueryResult

/** `commentTypes` accepts a single string or array of strings. */
export declare function removeComments(content: string, commentTypes: any): string

/**
 * Resolve a fuzzy symbol position (name + optional line hint) to an exact
 * line/character position inside the file at `file_path`.
 */
export declare function resolvePosition(filePath: string, fuzzy: JsFuzzyPosition): JsResolvedSymbol

/**
 * Resolve a fuzzy symbol position against in-memory `content` rather than
 * reading from disk. Use when the caller already holds the file text.
 */
export declare function resolvePositionFromContent(content: string, fuzzy: JsFuzzyPosition): JsResolvedSymbol

/** Walk upward from `file_path` to find the workspace root. */
export declare function resolveWorkspaceRootForFile(filePath: string): string

export interface RipgrepFile {
  path: string
  matchCount: number
  matches: Array<RipgrepMatch>
}

export interface RipgrepMatch {
  /** 1-based line number. */
  line: number
  /** 0-based column offset of the first submatch. */
  column: number
  /** Assembled match + context window, truncated to `max_snippet_chars`. */
  value: string
  /** Frequency for `onlyMatching + countUnique` values. */
  count?: number
  /**
   * AST node-kind label (declaration|import|export|callsite|identifier|comment|
   * string|configKey|heading) when `classifyMatches` is enabled.
   */
  kind?: string
  /** Deterministic relevance hint (0.0..1.0) derived from `kind`. */
  scoreHint?: number
}

export interface RipgrepParseOptions {
  /** Number of context lines around each match (default 0). */
  contextLines?: number
  /** Max Unicode chars per match snippet (default 500). */
  maxSnippetChars?: number
}

export interface RipgrepParseResult {
  files: Array<RipgrepFile>
  stats: RipgrepStats
}

export interface RipgrepPatternValidationResult {
  valid: boolean
  error?: string
}

/**
 * Options for the in-process ripgrep search (`searchRipgrep`). Field semantics
 * mirror the ripgrep CLI flags the old `RipgrepCommandBuilder` emitted.
 */
export interface RipgrepSearchOptions {
  /** Search root: a directory (recursive) or a single file. */
  path: string
  /** The search pattern (rg's positional pattern / `keywords`). */
  pattern: string
  /** Treat the pattern as a literal string, not a regex (`-F`). */
  fixedString?: boolean
  /** Use the PCRE2 engine for lookaround/backreferences (`-P`). */
  perlRegex?: boolean
  /** Case-sensitive match (`-s`). Wins over `caseInsensitive`. */
  caseSensitive?: boolean
  /** Case-insensitive match (`-i`). Default is smart-case (`-S`). */
  caseInsensitive?: boolean
  /** Match whole words only (`-w`). */
  wholeWord?: boolean
  /** Invert: report non-matching lines (`-v`). */
  invertMatch?: boolean
  /** Multi-line mode: `.` and the pattern may span lines (`-U`). */
  multiline?: boolean
  /** In multi-line mode, let `.` match newlines (`--multiline-dotall`). */
  multilineDotall?: boolean
  /** List only the paths of files that contain a match (`-l`). */
  filesOnly?: boolean
  /** List only the paths of files with no match (`--files-without-match`). */
  filesWithoutMatch?: boolean
  /** Per-file count of matching lines (`-c`). */
  countLinesPerFile?: boolean
  /** Per-file count of individual matches (`--count-matches`). */
  countMatchesPerFile?: boolean
  /** Context lines around each match (`-C`). */
  contextLines?: number
  /** Restrict to a ripgrep file type, e.g. `ts`, `py` (`-t`). */
  langType?: string
  /** Include globs (`-g <glob>`). */
  include?: Array<string>
  /** Exclude globs (`-g !<glob>`). */
  exclude?: Array<string>
  /** Exclude directories (`-g !<dir>/`). */
  excludeDir?: Array<string>
  /** Do not honor .gitignore/.ignore/etc. (`--no-ignore`). */
  noIgnore?: boolean
  /** Search hidden files and directories (`--hidden`). */
  hidden?: boolean
  /** Sort key: `path` (default), `modified`, `accessed`, or `created`. */
  sort?: string
  /** Reverse the sort order (`--sortr`). */
  sortReverse?: boolean
  /** Max Unicode chars per assembled snippet (default 500). */
  maxSnippetChars?: number
  /** Label each match with its AST node kind (tree-sitter) for ranking. */
  classifyMatches?: boolean
  /**
   * Emit one match per submatch with `value` set to the matched span (not the
   * whole line) — ripgrep's `-o`/`--only-matching`. Enumerates every hit on a
   * minified one-liner that line mode can only count.
   */
  onlyMatching?: boolean
  /**
   * With `onlyMatching`, collapse repeated matched values per file while
   * keeping the first occurrence anchor.
   */
  unique?: boolean
  /**
   * With `onlyMatching`, collapse repeated values per file and attach a
   * frequency count to each returned match. Sorted by count descending.
   */
  countUnique?: boolean
  /**
   * With `onlyMatching`, widen each span by this many characters on each side
   * (char-boundary safe), marking trimmed sides with `…`. 0/unset = bare span.
   */
  matchWindow?: number
}

export interface RipgrepStats {
  matchCount?: number
  matchedLines?: number
  filesMatched?: number
  filesSearched?: number
  bytesSearched?: number
  searchTime?: string
}

/**
 * Read `file_path` from disk after canonicalizing it and confirming it is an
 * absolute regular file.
 */
export declare function safeReadFile(filePath: string): string

export const SIGNATURES_ONLY_HINT: string

/**
 * Paginate `content` by char offset + length, with optional line-boundary
 * snapping. Replaces both the char-mode conversion block in `applyPagination`
 * and the dead-code `sliceByCharRespectLines` (0 callers confirmed by LSP).
 */
export declare function sliceContent(content: string, charOffset: number, charLength: number, options?: SliceContentOptions | undefined | null): SliceContentResult

export interface SliceContentOptions {
  /** When true, snap start to line start and end to line end (default false). */
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
 * Remove Python docstrings (module/class/function level) while preserving
 * all runtime code.
 */
export declare function stripPythonDocstrings(content: string): string

/**
 * One structural match. Line numbers are 1-based so `start_line` can be fed
 * directly as an `lspGetSemantics` `lineHint`; columns are 0-based char
 * offsets (tree-sitter native).
 */
/**
 * Precise position of one captured metavariable node. `line` is 1-based
 * (usable as an `lspGetSemantics` lineHint); columns are 0-based char offsets.
 */
export interface MetavarRange {
  text: string
  line: number
  column: number
  endLine: number
  endColumn: number
}

export interface StructuralMatch {
  startLine: number
  endLine: number
  startCol: number
  endCol: number
  text: string
  /**
   * Captured metavariables. `$X` yields a single-element list;
   * `$$$ARGS` yields the full list of captured nodes. Keyed by the bare
   * metavar name (no leading `$`).
   */
  metavars: Record<string, Array<string>>
  /**
   * Per-capture precise ranges, parallel to `metavars` (same keys/order).
   * Lets an agent hand a capture straight to LSP without re-searching.
   */
  metavarRanges: Record<string, Array<MetavarRange>>
}

export interface StructuralDiagnostic {
  code: string
  severity: 'info' | 'warning' | 'error' | string
  stage:
    | 'snapshot'
    | 'regionize'
    | 'scan'
    | 'parse'
    | 'match'
    | 'minify'
    | 'sanitize'
    | 'lsp'
    | 'paginate'
    | string
  message: string
  path?: string
  recovery?: string
}

export interface StructuralQueryExplanation {
  kind: 'pattern' | 'rule' | 'invalid' | string
  source: string
  literalAnchor?: string
  preFilter: 'literal-anchor' | 'disabled' | 'unavailable' | string
  unsafeReason?: string
  diagnostics: Array<StructuralDiagnostic>
}

export interface StructuralDetailedMatch extends StructuralMatch {
  id: string
  /** tree-sitter `kind` of the matched node (e.g. `call_expression`). */
  nodeKind?: string
  /**
   * The octo matcher is a precise AST matcher, so every match is an exact
   * tree-sitter node match — there is no partial/fallback tier.
   */
  confidence: 'exact-ast'
}

export interface StructuralSearchDetailedResult {
  path: string
  analyzer: string
  analyzerVersion: string
  status:
    | 'ok'
    | 'partial'
    | 'unsupported'
    | 'ambiguous'
    | 'parserFailed'
    | 'fallback'
    | 'truncated'
    | 'stale'
    | string
  languageId?: string
  query: StructuralQueryExplanation
  matches: Array<StructuralDetailedMatch>
  diagnostics: Array<StructuralDiagnostic>
}

/**
 * Structural (AST) search — octocode's L2 layer. Resolves the grammar from
 * `file_path`'s extension and matches an Octocode structural `pattern` OR a YAML `rule`
 * (exactly one). Returns node ranges (1-based lines, ready as `lineHint`s)
 * plus captured metavariables. Throws on unsupported extension, invalid
 * pattern/rule, or both/neither query supplied.
 *
 * Runs on libuv's worker pool (tree-sitter parse) — returns a Promise.
 */
export declare function structuralSearch(content: string, filePath: string, pattern?: string | undefined | null, rule?: string | undefined | null): Promise<Array<StructuralMatch>>

/**
 * Detailed structural search. Unsupported extensions and invalid queries return
 * status + diagnostics instead of being collapsed into a thrown legacy error.
 */
export declare function structuralSearchDetailed(content: string, filePath: string, pattern?: string | undefined | null, rule?: string | undefined | null): StructuralSearchDetailedResult

export interface StructuralSearchFileResult {
  path: string
  matches: Array<StructuralMatch>
}

/**
 * Runs on libuv's worker pool — the directory walk + per-file parse is
 * CPU/IO-bound and can span thousands of files. Returns a Promise.
 */
export declare function structuralSearchFiles(options: StructuralSearchFilesOptions): Promise<StructuralSearchFilesResult>

export declare function structuralSearchFilesDetailed(options: StructuralSearchFilesOptions): StructuralSearchFilesDetailedResult

export interface StructuralSearchFilesOptions {
  path: string
  pattern?: string
  rule?: string
  include?: Array<string>
  /**
   * File-path globs to skip (gitignore-style, e.g. `"*.min.js"`, `"src/gen/**"`).
   * Mirrors `localSearchCode.exclude` so OQL `scope.exclude` is honored on the
   * structural lane — previously silently dropped (typed-contract violation).
   */
  exclude?: Array<string>
  excludeDir?: Array<string>
  /**
   * Include hidden (dot) files. `None` preserves the default walker behavior
   * (hidden ignored); `Some(true)` forces them in.
   */
  hidden?: boolean
  /**
   * Bypass `.gitignore`/`.ignore` rules. `None` preserves defaults; `Some(true)`
   * searches files normally hidden by ignore files (mirrors `localSearchCode.noIgnore`).
   */
  noIgnore?: boolean
  /** Maximum directory descent depth (0 = just the root). `None` = unbounded. */
  maxDepth?: number
  maxFiles?: number
  maxFileBytes?: number
}

export interface StructuralSearchFilesResult {
  files: Array<StructuralSearchFileResult>
  totalMatches: number
  parsedFiles: number
  skippedByPreFilter: number
  /**
   * Candidate files whose extension has no grammar — not evaluated, hence
   * not proof of absence. Mirrors the detailed result's counter so the two
   * shapes agree and the warning text can't collapse unevaluated into
   * anchor-absent.
   */
  skippedUnsupported: number
  skippedUnreadable: number
  skippedLarge: number
  warnings: Array<string>
}

export interface StructuralSearchDetailedFileResult {
  path: string
  status: string
  languageId?: string
  skippedReason?: string
  matches: Array<StructuralDetailedMatch>
  diagnostics: Array<StructuralDiagnostic>
}

export interface StructuralSearchFilesDetailedResult {
  files: Array<StructuralSearchDetailedFileResult>
  totalMatches: number
  parsedFiles: number
  skippedByPreFilter: number
  skippedUnsupported: number
  skippedUnreadable: number
  skippedLarge: number
  analyzer: string
  analyzerVersion: string
  status: string
  query: StructuralQueryExplanation
  diagnostics: Array<StructuralDiagnostic>
  warnings: Array<string>
}

/**
 * Convert a human-readable symbol kind string back to the LSP `SymbolKind`
 * numeric code. Unknown strings return `13` (Variable).
 */
export declare function toLspSymbolKind(kind: string): number

/** Convert a filesystem path to a `file://` URI string. */
export declare function toUri(path: string): string

/** Validate that `command` resolves to an executable LSP server binary. */
export declare function validateLspServerPath(command: string): string

export declare function validateRipgrepPattern(pattern: string, fixedString?: boolean | undefined | null, perlRegex?: boolean | undefined | null): RipgrepPatternValidationResult

export interface YamlConversionConfig {
  sortKeys?: boolean
  keysPriority?: Array<string>
}

/** Result of secret detection + redaction over a string. */
export interface SanitizationResult {
  content: string
  hasSecrets: boolean
  secretsDetected: Array<string>
  warnings: Array<string>
}

/**
 * Detect and redact all secrets from `content`, returning the sanitized string
 * with `[REDACTED-*]` placeholders plus detection metadata. `filePath` gates
 * file-context patterns (e.g. Kubernetes/`.env` secrets).
 */
export declare function sanitizeContent(content: string, filePath?: string | undefined | null): SanitizationResult

/**
 * Mask secrets in place: every even-indexed char of a matched secret becomes
 * `*`, preserving partial readability. File-context patterns are skipped.
 */
export declare function maskSensitiveData(text: string): string

/** Number of loaded secret-detection patterns (testing / benchmarking). */
export declare function patternCount(): number
