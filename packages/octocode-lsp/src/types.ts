export type InitializationOptions = Record<string, unknown>;

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
