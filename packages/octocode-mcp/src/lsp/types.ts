export interface LanguageServerConfig {
  command: string;

  args?: string[];

  workspaceRoot: string;

  languageId?: string;
}

export interface UserLanguageServerConfig {
  command: string;

  args?: string[];

  languageId: string;
}

export interface LanguageServerCommand {
  command: string;

  args: string[];

  languageId: string;

  envVar: string;
}

export interface FuzzyPosition {
  symbolName: string;

  lineHint: number;

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

type LSPErrorType =
  | 'symbol_not_found'
  | 'file_not_found'
  | 'not_a_function'
  | 'lsp_unavailable'
  | 'timeout'
  | 'parse_error'
  | 'unknown';

interface LSPToolResultBase {
  status?: 'empty' | 'error';

  error?: string;

  errorType?: LSPErrorType;
  errorCode?: string;

  hints?: string[];

  [key: string]: unknown;
}

export interface GotoDefinitionResult extends LSPToolResultBase {
  locations?: CodeSnippet[];

  resolvedPosition?: ExactPosition;

  searchRadius?: number;

  outputPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
  };
}

export interface FindReferencesResult extends LSPToolResultBase {
  locations?: ReferenceLocation[];

  byFile?: ReferencesByFile[];

  totalReferences?: number;

  totalFiles?: number;

  pagination?: LSPPaginationInfo;

  hasMultipleFiles?: boolean;
}

export interface CallHierarchyResult extends LSPToolResultBase {
  item?: CallHierarchyItem;

  incomingCalls?: IncomingCall[];

  outgoingCalls?: OutgoingCall[];

  pagination?: LSPPaginationInfo;

  outputPagination?: {
    charOffset: number;
    charLength: number;
    totalChars: number;
    hasMore: boolean;
    currentPage: number;
    totalPages: number;
  };

  direction?: 'incoming' | 'outgoing';

  depth?: number;
}
