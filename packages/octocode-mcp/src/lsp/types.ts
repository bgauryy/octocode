/**
 * LSP (Language Server Protocol) types for octocode-mcp
 * Provides semantic code intelligence features
 * @module lsp/types
 */

/**
 * Language server configuration for spawning a server
 */
export interface LanguageServerConfig {
  /** Command to spawn the language server */
  command: string;
  /** Arguments for the command */
  args?: string[];
  /** Working directory (workspace root) */
  workspaceRoot: string;
  /** Language ID (typescript, javascript, python, go, etc.) */
  languageId?: string;
}

/**
 * User-configurable language server entry
 */
export interface UserLanguageServerConfig {
  /** Command to spawn the language server */
  command: string;
  /** Arguments for the command (default: []) */
  args?: string[];
  /** Language ID for the server (e.g., 'python', 'go') */
  languageId: string;
}

/**
 * Config file schema for user-defined language servers
 * File locations (in priority order):
 * 1. OCTOCODE_LSP_CONFIG environment variable
 * 2. .octocode/lsp-servers.json (workspace-level)
 * 3. ${OCTOCODE_HOME:-~/.octocode}/lsp-servers.json (user-level)
 */
export interface LSPConfigFile {
  /** Language servers by file extension (e.g., ".py", ".java") */
  languageServers?: Record<string, UserLanguageServerConfig>;
}

/**
 * Server command info from the registry
 */
export interface LanguageServerCommand {
  /** Command to spawn the language server */
  command: string;
  /** Arguments for the command */
  args: string[];
  /** Language ID for the server */
  languageId: string;
  /** Environment variable to override the command */
  envVar: string;
}

/**
 * Fuzzy position for symbol resolution
 * Used when exact character position is not known
 */
export interface FuzzyPosition {
  /** EXACT symbol name to find (case-sensitive, no partial matches) */
  symbolName: string;
  /** 1-indexed line number hint (tool searches ±radius lines) */
  lineHint: number;
  /** 0-indexed occurrence if symbol appears multiple times on same line */
  orderHint?: number;
}

/**
 * Exact position in a file (0-indexed for LSP compatibility)
 */
export interface ExactPosition {
  /** 0-indexed line number */
  line: number;
  /** 0-indexed character position */
  character: number;
}

/**
 * Range in a file (0-indexed)
 */
export interface LSPRange {
  start: ExactPosition;
  end: ExactPosition;
}

/**
 * Code snippet with location information
 */
export interface CodeSnippet {
  /** File path (relative to workspace root) */
  uri: string;
  /** Range in the file */
  range: LSPRange;
  /** Code content */
  content: string;
  /** Symbol kind (function, class, variable, type, etc.) */
  symbolKind?: SymbolKind;
  /** 1-indexed line numbers for display */
  displayRange?: {
    startLine: number;
    endLine: number;
  };
}

/**
 * Symbol kinds for LSP
 */
export type SymbolKind =
  | 'function'
  | 'method'
  | 'class'
  | 'interface'
  | 'type'
  | 'variable'
  | 'constant'
  | 'property'
  | 'enum'
  | 'module'
  | 'namespace'
  | 'unknown';

/**
 * Reference location with context
 */
export interface ReferenceLocation extends CodeSnippet {
  /** Whether this is the definition (vs a usage) */
  isDefinition?: boolean;
}

/**
 * Call hierarchy item
 */
export interface CallHierarchyItem {
  /** Function/method name */
  name: string;
  /** Symbol kind */
  kind: SymbolKind;
  /** File containing the function */
  uri: string;
  /** Range of the function definition */
  range: LSPRange;
  /** Selection range (usually the function name) */
  selectionRange?: LSPRange;
  /** Code content around the call site */
  content?: string;
  /** 1-indexed line numbers for display */
  displayRange?: {
    startLine: number;
    endLine: number;
  };
}

/**
 * Incoming call (who calls this function)
 */
export interface IncomingCall {
  /** The function that makes the call */
  from: CallHierarchyItem;
  /** Ranges where calls are made from 'from' to this function */
  fromRanges: LSPRange[];
}

/**
 * Outgoing call (what this function calls)
 */
export interface OutgoingCall {
  /** The function being called */
  to: CallHierarchyItem;
  /** Ranges where calls are made from this function to 'to' */
  fromRanges: LSPRange[];
}

/**
 * Pagination info for LSP results
 */
export interface LSPPaginationInfo {
  currentPage: number;
  totalPages: number;
  totalResults: number;
  hasMore: boolean;
  resultsPerPage: number;
}

/**
 * LSP error types for dynamic hints
 */
type LSPErrorType =
  | 'symbol_not_found'
  | 'file_not_found'
  | 'not_a_function'
  | 'timeout'
  | 'parse_error'
  | 'unknown';

/**
 * Source of a result's locations/calls.
 *
 * - `'semantic'`: derived from a real Language Server (cross-file refs,
 *   import chasing, type-aware results).
 * - `'fallback'`: derived from text/pattern matching only — caller MUST
 *   treat results as best-effort and verify renamed/aliased usages
 *   another way.
 *
 * This is the structured equivalent of `LSP_UNAVAILABLE_HINT`. It is a
 * top-level field (not buried inside `hints[]`) so it survives
 * char-budget pagination and lets agents branch on it without parsing
 * hint strings.
 */
export type LspMode = 'semantic' | 'fallback';

/**
 * Base LSP tool result
 */
interface LSPToolResultBase {
  /** Result status */
  status: 'hasResults' | 'empty' | 'error';
  /** Error message if status is 'error' */
  error?: string;
  /** Error type for hint generation (lowercase, internal) */
  errorType?: LSPErrorType;
  /**
   * Stable agent-facing error code (SCREAMING_SNAKE_CASE). Wire-stable
   * across releases; safe to switch on. See {@link ../lsp/lspErrorCodes}.
   */
  errorCode?: string;
  /** Hints for next steps */
  hints?: string[];
  /**
   * Whether locations/calls came from semantic LSP analysis or from
   * pattern-matching fallback. Absent on error/empty results that did
   * not consult either source.
   */
  lspMode?: LspMode;
  /** Index signature for ProcessedBulkResult compatibility */
  [key: string]: unknown;
}

/**
 * Go to definition result
 */
export interface GotoDefinitionResult extends LSPToolResultBase {
  /** Definition locations (usually 1, but can be multiple for overloads) */
  locations?: CodeSnippet[];
  /** Resolved position where symbol was found */
  resolvedPosition?: ExactPosition;
  /** Search radius used */
  searchRadius?: number;
  /** Output pagination metadata when response exceeds size limits */
  outputPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
  };
}

/**
 * Find references result
 */
export interface FindReferencesResult extends LSPToolResultBase {
  /** Reference locations */
  locations?: ReferenceLocation[];
  /** Pagination info */
  pagination?: LSPPaginationInfo;

  /** Whether references span multiple files */
  hasMultipleFiles?: boolean;
}

/**
 * Call hierarchy result
 */
export interface CallHierarchyResult extends LSPToolResultBase {
  /** The target function for hierarchy */
  item?: CallHierarchyItem;
  /** Incoming calls (when direction='incoming') */
  incomingCalls?: IncomingCall[];
  /** Outgoing calls (when direction='outgoing') */
  outgoingCalls?: OutgoingCall[];
  /** Pagination info */
  pagination?: LSPPaginationInfo;
  /** Character-based output pagination (when output exceeds size limit) */
  outputPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
  };
  /** Direction of the hierarchy search */
  direction?: 'incoming' | 'outgoing';
  /** Depth of the search */
  depth?: number;
}
