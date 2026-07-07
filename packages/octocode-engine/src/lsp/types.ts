export type InitializationOptions = Record<string, unknown>;

/**
 * Outcome of `waitForReady` — a readiness signal that lets a zero-results
 * semantic query be interpreted honestly:
 *   - `progressIdle`   — a `$/progress` indexing cycle was seen and drained to
 *     idle; an empty answer reflects the fully-indexed project.
 *   - `settledFallback`— no `$/progress` was ever seen, only the fixed settle
 *     window elapsed; indexing completion is unconfirmed.
 *   - `timeout`        — `$/progress` was still active when the deadline hit;
 *     the server is (as far as we know) still indexing.
 */
export type LspReadiness = 'progressIdle' | 'settledFallback' | 'timeout';

/**
 * Where a language-server executable was resolved from, for honest status
 * reporting. Ordered by the resolution ladder (see docs/context/LSP_GUIDE.md):
 * explicit override / PATH → bundled JS dep → project-local → ecosystem dir →
 * managed download cache → IDE bridge; `unavailable` when nothing provides it.
 */
export type LspServerSource =
  | 'path'
  | 'bundled'
  | 'project-local'
  | 'ecosystem'
  | 'managed-cache'
  | 'ide-bridge'
  | 'unavailable';

export interface LanguageServerConfig {
  command: string;

  args?: string[];

  workspaceRoot: string;

  languageId?: string;

  initializationOptions?: InitializationOptions;

  env?: Record<string, string>;
}

export interface UserLanguageServerConfig {
  command: string;

  args?: string[];

  languageId: string;

  initializationOptions?: InitializationOptions;
}

export interface LanguageServerCommand {
  command: string;

  args: string[];

  languageId: string;

  envVar: string;

  packageName?: string;

  binName?: string;
}

export interface FuzzyPosition {
  symbolName: string;

  lineHint?: number;

  orderHint?: number;
}

export interface ExactPosition {
  line: number;

  character: number;
}

export interface LSPRange {
  start: ExactPosition;
  end: ExactPosition;
}

export interface CodeSnippet {
  uri: string;

  range: LSPRange;

  content: string;

  symbolKind?: SymbolKind;

  displayRange?: {
    startLine: number;
    endLine: number;
  };
}

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

export interface ReferenceLocation extends CodeSnippet {
  isDefinition?: boolean;
}

export interface ReferencesByFile {
  uri: string;

  count: number;

  firstLine: number;

  firstCharacter: number;

  lines: number[];

  hasDefinition?: boolean;
}

export interface CallHierarchyItem {
  name: string;

  kind: SymbolKind;

  uri: string;

  range: LSPRange;

  selectionRange?: LSPRange;

  content?: string;

  displayRange?: {
    startLine: number;
    endLine: number;
  };
}

export interface IncomingCall {
  from: CallHierarchyItem;

  fromRanges: LSPRange[];
}

export interface OutgoingCall {
  to: CallHierarchyItem;

  fromRanges: LSPRange[];
}

export interface LSPPaginationInfo {
  currentPage: number;
  totalPages: number;
  totalResults: number;
  hasMore: boolean;
  resultsPerPage?: number;
}
